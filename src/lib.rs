#![forbid(unsafe_code, future_incompatible)]

use async_session::{async_trait, serde_json, Result, Session, SessionStore};
use fred::{
    pool::RedisPool,
    prelude::*,
    types::{RedisKey, ScanType},
};
use futures::stream::StreamExt;

#[derive(Clone)]
pub struct RedisSessionStore {
    pool: RedisPool,
    prefix: Option<String>,
}

impl std::fmt::Debug for RedisSessionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.prefix)
    }
}

impl RedisSessionStore {
    pub fn from_pool(pool: RedisPool, prefix: Option<String>) -> Self {
        Self { pool, prefix }
    }

    pub async fn count(&self) -> Result<usize> {
        match self.prefix {
            None => Ok(self.pool.dbsize().await?),
            Some(_) => Ok(self.ids().await?.map_or(0, |v| v.len())),
        }
    }

    async fn ids(&self) -> Result<Option<Vec<RedisKey>>> {
        let mut result = Vec::new();
        let mut scanner = self
            .pool
            .scan(self.prefix_key("*"), None, Some(ScanType::String));

        while let Some(res) = scanner.next().await {
            if let Some(keys) = res?.take_results() {
                result.extend_from_slice(&keys);
            }
        }

        Ok((!result.is_empty()).then_some(result))
    }

    fn prefix_key(&self, key: &str) -> String {
        match &self.prefix {
            None => key.to_string(),
            Some(prefix) => format!("{prefix}{key}"),
        }
    }

    #[cfg(test)]
    async fn ttl_for_session(&self, session: &Session) -> Result<usize> {
        Ok(self.pool.ttl(self.prefix_key(session.id())).await?)
    }
}

#[async_trait]
impl SessionStore for RedisSessionStore {
    async fn load_session(&self, cookie_value: String) -> Result<Option<Session>> {
        let id = Session::id_from_cookie_value(&cookie_value)?;
        Ok(self
            .pool
            .get::<Option<String>, String>(self.prefix_key(&id))
            .await?
            .map(|v| serde_json::from_str(&v))
            .transpose()?)
    }

    async fn store_session(&self, session: Session) -> Result<Option<String>> {
        let id = self.prefix_key(session.id());
        let string = serde_json::to_string(&session)?;
        let expiration = session
            .expires_in()
            .map(|d| Expiration::EX(d.as_secs() as i64));

        self.pool.set(id, string, expiration, None, false).await?;

        Ok(session.into_cookie_value())
    }

    async fn destroy_session(&self, session: Session) -> Result {
        Ok(self.pool.del(self.prefix_key(session.id())).await?)
    }

    async fn clear_store(&self) -> Result {
        match self.prefix {
            None => Ok(self.pool.flushall(false).await?),
            Some(_) => match self.ids().await? {
                None => Ok(()),
                Some(ids) => Ok(self.pool.del(ids).await?),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;
    use tokio::time::sleep;

    async fn create_session_store() -> RedisSessionStore {
        let conf = RedisConfig::from_url("redis://127.0.0.1:6379").unwrap();
        let pool = RedisPool::new(conf, 6).unwrap();

        pool.connect(None);
        pool.wait_for_connect().await.unwrap();

        let store = RedisSessionStore::from_pool(pool, Some("async-session-test/".into()));
        store.clear_store().await.unwrap();
        store
    }

    #[tokio::test]
    async fn creating_a_new_session_with_no_expiry() -> Result {
        let store = create_session_store().await;
        let mut session = Session::new();
        session.insert("key", "Hello")?;

        let cloned = session.clone();
        let cookie_value = store.store_session(session).await?.unwrap();
        let loaded_session = store.load_session(cookie_value).await?.unwrap();

        assert_eq!(cloned.id(), loaded_session.id());
        assert_eq!("Hello", &loaded_session.get::<String>("key").unwrap());
        assert!(!loaded_session.is_expired());
        assert!(loaded_session.validate().is_some());

        Ok(())
    }

    #[tokio::test]
    async fn updating_a_session() -> Result {
        let store = create_session_store().await;
        let mut session = Session::new();

        session.insert("key", "value")?;
        let cookie_value = store.store_session(session).await?.unwrap();
        let mut session = store.load_session(cookie_value.clone()).await?.unwrap();

        session.insert("key", "other value")?;
        assert_eq!(None, store.store_session(session).await?);
        let session = store.load_session(cookie_value.clone()).await?.unwrap();

        assert_eq!(&session.get::<String>("key").unwrap(), "other value");
        assert_eq!(1, store.count().await.unwrap());

        Ok(())
    }

    #[tokio::test]
    async fn updating_a_session_extending_expiry() -> Result {
        let store = create_session_store().await;
        let mut session = Session::new();
        session.expire_in(Duration::from_secs(5));
        let original_expires = session.expiry().unwrap().clone();
        let cookie_value = store.store_session(session).await?.unwrap();

        let mut session = store.load_session(cookie_value.clone()).await?.unwrap();
        let ttl = store.ttl_for_session(&session).await?;
        assert!(ttl > 3 && ttl < 5);

        assert_eq!(session.expiry().unwrap(), &original_expires);
        session.expire_in(Duration::from_secs(10));
        let new_expires = session.expiry().unwrap().clone();
        store.store_session(session).await?;

        let session = store.load_session(cookie_value.clone()).await?.unwrap();
        let ttl = store.ttl_for_session(&session).await?;
        assert!(ttl > 8 && ttl < 10);
        assert_eq!(session.expiry().unwrap(), &new_expires);

        assert_eq!(1, store.count().await.unwrap());
        sleep(Duration::from_secs(10)).await;
        assert_eq!(0, store.count().await.unwrap());

        Ok(())
    }

    #[tokio::test]
    async fn creating_a_new_session_with_expiry() -> Result {
        let store = create_session_store().await;
        let mut session = Session::new();
        session.expire_in(Duration::from_secs(3));
        session.insert("key", "value")?;
        let cloned = session.clone();

        let cookie_value = store.store_session(session).await?.unwrap();

        assert!(store.ttl_for_session(&cloned).await? > 1);

        let loaded_session = store.load_session(cookie_value.clone()).await?.unwrap();
        assert_eq!(cloned.id(), loaded_session.id());
        assert_eq!("value", &loaded_session.get::<String>("key").unwrap());

        assert!(!loaded_session.is_expired());

        sleep(Duration::from_secs(2)).await;
        assert_eq!(None, store.load_session(cookie_value).await?);

        Ok(())
    }

    #[tokio::test]
    async fn destroying_a_single_session() -> Result {
        let store = create_session_store().await;
        for _ in 0..3 {
            store.store_session(Session::new()).await?;
        }

        let cookie = store.store_session(Session::new()).await?.unwrap();
        assert_eq!(4, store.count().await?);
        let session = store.load_session(cookie.clone()).await?.unwrap();
        store.destroy_session(session.clone()).await.unwrap();
        assert_eq!(None, store.load_session(cookie).await?);
        assert_eq!(3, store.count().await?);

        // attempting to destroy the session again is not an error
        assert!(store.destroy_session(session).await.is_ok());
        Ok(())
    }

    #[tokio::test]
    async fn clearing_the_whole_store() -> Result {
        let store = create_session_store().await;
        for _ in 0..3 {
            store.store_session(Session::new()).await?;
        }

        assert_eq!(3, store.count().await?);
        store.clear_store().await.unwrap();
        assert_eq!(0, store.count().await?);

        Ok(())
    }
}

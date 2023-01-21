#![forbid(unsafe_code, future_incompatible)]
#![allow(dead_code, unused_imports, unused_variables)]

use async_session::{async_trait, serde_json, Result, Session, SessionStore};
use fred::{
    clients::RedisClient,
    pool::RedisPool,
    prelude::*,
    types::{RedisConfig, RedisKey, ScanResult, ScanType},
};
use futures::stream::StreamExt;

#[derive(Clone)]
pub struct RedisSessionStore {
    pool: RedisPool,
    prefix: Option<String>,
}

impl RedisSessionStore {
    pub fn from_pool(pool: RedisPool, prefix: Option<String>) -> Self {
        Self { pool, prefix }
    }

    pub async fn count(&self) -> Result<usize> {
        if self.prefix.is_none() {
            Ok(self.pool.dbsize().await?)
        } else {
            Ok(self.ids().await?.len())
        }
    }

    async fn ids(&self) -> Result<Vec<RedisKey>> {
        let results = self
            .pool
            .scan(self.prefix_key("*"), None, None)
            .collect::<Vec<_>>()
            .await;
        let mut rds_keys = Vec::new();

        for res in results.into_iter() {
            if let Some(keys) = res?.take_results() {
                rds_keys.extend_from_slice(&keys)
            }
        }

        Ok(rds_keys)
    }

    fn prefix_key(&self, key: &str) -> String {
        match &self.prefix {
            None => key.to_string(),
            Some(prefix) => format!("{prefix}{key}"),
        }
    }
}

// #[async_trait]
// impl SessionStore for RedisSessionStore {
//     async fn load_session(&self, cookie_value: String) -> Result<Option<Session>> {}
//     async fn store_session(&self, session: Session) -> Result<Option<String>> {}
//     async fn destroy_session(&self, session: Session) -> Result {}
//     async fn clear_store(&self) -> Result {}
// }

#[cfg(test)]
mod tests {
    use super::*;

    async fn create_session_store() -> RedisSessionStore {
        let conf = RedisConfig::from_url("redis://127.0.0.1:6379").unwrap();
        let pool = RedisPool::new(conf, 6).unwrap();

        pool.connect(None);
        pool.wait_for_connect().await.unwrap();

        RedisSessionStore::from_pool(pool, Some("async-session-test/".into()))
    }

    // #[tokio::test]
    // async fn creating_a_new_session_with_no_expiry() -> Result {
    //     let store = create_session_store();
    //     let mut session = Session::new();
    //     session.insert("key", "Hello")?;
    //     let cloned = session.clone();
    //     let cookie_value = store.store_session(session).await?.unwrap();
    //     let loaded_session = store.load_session(cookie_value).await?.unwrap();
    //     assert_eq!(cloned.id(), loaded_session.id());
    //     assert_eq!("Hello", &loaded_session.get::<String>("key").unwrap());
    //     assert!(!loaded_session.is_expired());
    //     assert!(loaded_session.validate().is_some());
    //     Ok(())
    // }
}

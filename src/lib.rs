#![forbid(unsafe_code, future_incompatible)]
#![allow(dead_code, unused_imports, unused_variables)]

use async_session::{async_trait, serde_json, Session, SessionStore};
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

    async fn ids(&self) -> Option<Vec<RedisKey>> {
        let mut results = Vec::new();
        let mut scanner = self
            .pool
            .scan(self.prefix_key("*"), None, Some(ScanType::String));

        while let Some(Ok(page)) = scanner.next().await {
            if let Some(keys) = page.results() {
                results.extend_from_slice(&keys);
            }
        }

        if results.is_empty() {
            None
        } else {
            Some(results)
        }
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

# async-fred-session

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg?style=flat-square)](LICENSE.md)
[![docs.rs](https://img.shields.io/badge/docs-latest-blue.svg?style=flat-square)](https://docs.rs/async-fred-session)
[![crates.io](https://img.shields.io/crates/v/async-fred-session.svg)](https://crates.io/crates/async-fred-session)

Redis backed session store for [async-session](https://github.com/http-rs/async-session) using [fred.rs](https://github.com/aembke/fred.rs).

```rust
use async_fred_session::{RedisSessionStore, fred::{pool::RedisPool, types::RedisConfig}};
use async_session::{Session, SessionStore};

// pool creation
let config = RedisConfig::from_url("redis://127.0.0.1:6379").unwrap();
let rds_pool = RedisPool::new(config, None, None, 6).unwrap();
rds_pool.connect();
rds_pool.wait_for_connect().await.unwrap();

// store and session
let store = RedisSessionStore::from_pool(rds_pool, Some("async-fred-session/".into()));
let mut session = Session::new();
session.insert("key", "value").unwrap();

let cookie_value = store.store_session(session).await.unwrap().unwrap();
let session = store.load_session(cookie_value).await.unwrap().unwrap();
assert_eq!(&session.get::<String>("key").unwrap(), "value");
```

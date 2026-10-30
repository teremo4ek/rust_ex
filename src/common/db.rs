use std::str::FromStr;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::common::error::AppResult;

pub async fn connect(url: &str) -> AppResult<SqlitePool> {
    let options = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true);

    // a :memory: database is private to each connection, so the pool must pin
    // exactly one immortal connection or the schema silently resets
    let in_memory = url.contains(":memory:") || url.contains("mode=memory");
    let mut builder = SqlitePoolOptions::new();
    if in_memory {
        builder = builder
            .max_connections(1)
            .idle_timeout(None)
            .max_lifetime(None);
    } else {
        builder = builder.max_connections(5);
    }

    let pool = builder.connect_with(options).await?;
    sqlx::migrate!().run(&pool).await?;
    Ok(pool)
}

use crate::common::config::Config;
use crate::common::db;
use crate::common::error::AppResult;

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    pub jwt_expires_in: i64,
}

impl AppState {
    pub async fn from_config(config: &Config) -> AppResult<Self> {
        let pool = db::connect(&config.database_url).await?;
        Ok(Self {
            pool,
            jwt_secret: config.jwt_secret.clone(),
            jwt_expires_in: config.jwt_expires_in,
        })
    }
}

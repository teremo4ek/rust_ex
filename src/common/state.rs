use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::common::config::Config;
use crate::common::db;
use crate::common::error::AppResult;
use crate::common::models::{Expense, User};

#[derive(Debug, Clone)]
pub struct AppState {
    pub pool: sqlx::SqlitePool,
    pub jwt_secret: String,
    pub jwt_expires_in: i64,
    pub users: Arc<RwLock<HashMap<uuid::Uuid, User>>>,
    pub expenses: Arc<RwLock<Vec<Expense>>>,
}

impl AppState {
    pub async fn from_config(config: &Config) -> AppResult<Self> {
        let pool = db::connect(&config.database_url).await?;
        Ok(Self {
            pool,
            jwt_secret: config.jwt_secret.clone(),
            jwt_expires_in: config.jwt_expires_in,
            users: Arc::new(RwLock::new(HashMap::new())),
            expenses: Arc::new(RwLock::new(Vec::new())),
        })
    }
}

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::config::Config;
use crate::models::{Expense, User};

#[derive(Debug, Clone)]
pub struct AppState {
    pub users: Arc<RwLock<HashMap<uuid::Uuid, User>>>,
    pub expenses: Arc<RwLock<Vec<Expense>>>,
    pub jwt_secret: String,
    pub jwt_expires_in: i64,
}

impl AppState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            users: Arc::new(RwLock::new(HashMap::new())),
            expenses: Arc::new(RwLock::new(Vec::new())),
            jwt_secret: config.jwt_secret.clone(),
            jwt_expires_in: config.jwt_expires_in,
        }
    }
}

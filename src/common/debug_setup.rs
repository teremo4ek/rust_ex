use uuid::Uuid;

use crate::common::services::auth_service;
use crate::common::state::AppState;

pub const DEBUG_USER_ID: Uuid = Uuid::from_u128(1);
pub const DEBUG_EMAIL: &str = "debug@example.com";
pub const DEBUG_PASSWORD: &str = "debug123";
pub const DEBUG_JWT_SECRET: &str = "debug_secret_do_not_use_in_production";
const DEBUG_TOKEN_EXPIRES_IN: i64 = 10 * 365 * 24 * 60 * 60;

pub async fn seed_debug_user(state: &AppState) {
    let password_hash = auth_service::hash_password(DEBUG_PASSWORD).unwrap();
    // bare DO NOTHING covers both id and email conflicts without failing
    sqlx::query("INSERT INTO users (id, email, password_hash) VALUES (?, ?, ?) ON CONFLICT DO NOTHING")
        .bind(DEBUG_USER_ID.to_string())
        .bind(DEBUG_EMAIL)
        .bind(password_hash)
        .execute(&state.pool)
        .await
        .unwrap();
}

pub fn print_debug_banner() {
    let token = auth_service::generate_token(
        &DEBUG_USER_ID.to_string(),
        DEBUG_JWT_SECRET,
        DEBUG_TOKEN_EXPIRES_IN,
    )
    .unwrap();

    tracing::info!("=== DEBUG MODE ===");
    tracing::info!("email:    {}", DEBUG_EMAIL);
    tracing::info!("password: {}", DEBUG_PASSWORD);
    tracing::info!("token:    {}", token);
    tracing::info!("=================");
}

use uuid::Uuid;

use expense_tracker::models::User;
use expense_tracker::services::auth_service;
use expense_tracker::state::AppState;

pub const DEBUG_USER_ID: Uuid = Uuid::from_u128(1);
pub const DEBUG_EMAIL: &str = "debug@example.com";
pub const DEBUG_PASSWORD: &str = "debug123";
pub const DEBUG_JWT_SECRET: &str = "debug_secret_do_not_use_in_production";
const DEBUG_TOKEN_EXPIRES_IN: i64 = 10 * 365 * 24 * 60 * 60;

pub async fn seed_debug_user(state: &AppState) {
    let mut users = state.users.write().await;

    if !users.contains_key(&DEBUG_USER_ID) {
        let password_hash = auth_service::hash_password(DEBUG_PASSWORD).unwrap();
        let user = User {
            id: DEBUG_USER_ID,
            email: DEBUG_EMAIL.to_string(),
            password_hash,
        };
        users.insert(user.id, user);
    }
    drop(users);

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

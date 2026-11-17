use std::collections::BTreeSet;

use axum::Router;
use chrono::NaiveDate;
use rust_decimal::Decimal;

use expense_tracker::common::config::Config;
use expense_tracker::common::services::{auth_service, expense_service};
use expense_tracker::common::state::AppState;
use expense_tracker::web::routes;

pub async fn create_test_app() -> Router {
    create_test_app_with_state().await.0
}

pub async fn create_test_app_with_state() -> (Router, AppState) {
    let config = Config {
        jwt_secret: "test_secret".into(),
        jwt_expires_in: 3600,
        port: 0,
        database_url: "sqlite::memory:".into(),
    };
    let state = AppState::from_config(&config).await.unwrap();
    let app = routes::create_router(state.clone())
        .route("/health", axum::routing::get(|| async { "ok" }));
    (app, state)
}

#[allow(dead_code)]
pub async fn seed_rate(state: &AppState, date: NaiveDate, rate: Decimal) {
    sqlx::query(
        "INSERT OR REPLACE INTO nbrb_rates (date, currency, base_currency, rate)
         VALUES (?, 'USD', 'BYN', ?)",
    )
    .bind(date)
    .bind(rate.to_string())
    .execute(&state.pool)
    .await
    .unwrap();
}

#[allow(dead_code)]
pub async fn seed_fixture_rates(state: &AppState, rate: Decimal) {
    let expenses =
        expense_service::parse_csv(include_str!("../fixtures/monefy.csv"), uuid::Uuid::new_v4())
            .unwrap();
    let dates: BTreeSet<NaiveDate> = expenses.iter().map(|e| e.date).collect();
    for date in dates {
        seed_rate(state, date, rate).await;
    }
}

pub async fn register_and_login(state: &AppState) -> (String, uuid::Uuid) {
    let (token, user) = auth_service::register(
        state,
        "test@example.com".into(),
        "password123".into(),
        &state.jwt_secret,
        state.jwt_expires_in,
    )
    .await
    .unwrap();

    (token, user.id)
}

#[allow(dead_code)]
pub fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

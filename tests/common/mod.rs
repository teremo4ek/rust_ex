use std::collections::BTreeSet;

use axum::Router;
use axum::body::Body;
use chrono::NaiveDate;
use rust_decimal::Decimal;
use serde_json::{Value, json};
use tower::ServiceExt;

use expense_tracker::common::config::Config;
use expense_tracker::common::services::expense_service;
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
pub async fn seed_fixture_rates(state: &AppState, rate: Decimal) {
    let expenses =
        expense_service::parse_csv(include_str!("../fixtures/monefy.csv"), uuid::Uuid::new_v4())
            .unwrap();
    let dates: BTreeSet<NaiveDate> = expenses.iter().map(|e| e.date).collect();
    for date in dates {
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
}

pub async fn register_and_login(app: &Router) -> (String, uuid::Uuid) {
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(
                    serde_json::to_vec(
                        &json!({"email": "test@example.com", "password": "password123"}),
                    )
                    .unwrap(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), 201);

    let body: Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    let token = body["token"].as_str().unwrap().to_string();
    let user_id = body["user"]["id"].as_str().unwrap().parse().unwrap();

    (token, user_id)
}

#[allow(dead_code)]
pub fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

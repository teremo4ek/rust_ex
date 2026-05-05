use axum::body::Body;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use expense_tracker::config::Config;
use expense_tracker::routes;
use expense_tracker::state::AppState;

pub fn create_test_app() -> Router {
    let config = Config {
        jwt_secret: "test_secret".into(),
        jwt_expires_in: 3600,
        port: 0,
    };
    let state = AppState::from_config(&config);
    routes::create_router(state).route("/health", axum::routing::get(|| async { "ok" }))
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

pub fn auth_header(token: &str) -> String {
    format!("Bearer {}", token)
}

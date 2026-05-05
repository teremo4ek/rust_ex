mod common;

use axum::body::Body;
use axum::http::StatusCode;
use serde_json::json;
use tower::ServiceExt;

fn json_body(value: serde_json::Value) -> Body {
    Body::from(serde_json::to_vec(&value).unwrap())
}

#[tokio::test]
async fn test_register() {
    let app = common::create_test_app();

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/register")
                .header("content-type", "application/json")
                .body(json_body(json!({
                    "email": "new@example.com",
                    "password": "password123"
                })))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body["token"].is_string());
    assert_eq!(body["user"]["email"], "new@example.com");
}

#[tokio::test]
async fn test_register_duplicate_email() {
    let app = common::create_test_app();

    let req_body = json!({"email": "dup@example.com", "password": "password123"});
    let body_bytes = serde_json::to_vec(&req_body).unwrap();

    let req = axum::http::Request::builder()
        .method("POST")
        .uri("/api/auth/register")
        .header("content-type", "application/json")
        .body(Body::from(body_bytes.clone()))
        .unwrap();

    let res = app.clone().oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::CREATED);

    let res2 = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/register")
                .header("content-type", "application/json")
                .body(Body::from(body_bytes))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res2.status(), StatusCode::CONFLICT);
}

#[tokio::test]
async fn test_register_invalid_email() {
    let app = common::create_test_app();

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/register")
                .header("content-type", "application/json")
                .body(json_body(json!({
                    "email": "not-an-email",
                    "password": "password123"
                })))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_register_short_password() {
    let app = common::create_test_app();

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/register")
                .header("content-type", "application/json")
                .body(json_body(json!({
                    "email": "test@example.com",
                    "password": "short"
                })))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn test_login() {
    let app = common::create_test_app();
    common::register_and_login(&app).await;
}

#[tokio::test]
async fn test_login_wrong_password() {
    let app = common::create_test_app();
    let _ = common::register_and_login(&app).await;

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(json_body(json!({
                    "email": "test@example.com",
                    "password": "wrong"
                })))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let app = common::create_test_app();

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(json_body(json!({
                    "email": "nope@example.com",
                    "password": "password123"
                })))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_unauthorized_access() {
    let app = common::create_test_app();

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/expenses")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_health_check() {
    let app = common::create_test_app();

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

mod common;

use axum::body::Body;
use axum::http::StatusCode;
use serde_json::json;
use tower::ServiceExt;

fn json_body(value: serde_json::Value) -> Body {
    Body::from(serde_json::to_vec(&value).unwrap())
}

#[tokio::test]
async fn test_login() {
    let (app, state) = common::create_test_app_with_state().await;
    common::register_and_login(&state).await;

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/auth/login")
                .header("content-type", "application/json")
                .body(json_body(json!({
                    "email": "test@example.com",
                    "password": "password123"
                })))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_login_wrong_password() {
    let (app, state) = common::create_test_app_with_state().await;
    let _ = common::register_and_login(&state).await;

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
async fn test_register_endpoint_removed() {
    let app = common::create_test_app().await;

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

    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_login_nonexistent_user() {
    let app = common::create_test_app().await;

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
    let app = common::create_test_app().await;

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
    let app = common::create_test_app().await;

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

#[tokio::test]
async fn test_root_serves_landing_page() {
    let app = common::create_test_app().await;

    let res = app
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    assert!(res
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .expect("content-type header")
        .to_str()
        .unwrap()
        .starts_with("text/html"));

    let body = axum::body::to_bytes(res.into_body(), usize::MAX)
        .await
        .unwrap();
    let body = String::from_utf8(body.to_vec()).unwrap();
    assert!(body.contains("Expense Tracker"));
}

#[tokio::test]
async fn test_static_assets() {
    let app = common::create_test_app().await;

    for (uri, prefix) in [("/app.js", "text/javascript"), ("/style.css", "text/css")] {
        let res = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .method("GET")
                    .uri(uri)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(res.status(), StatusCode::OK);
        assert!(res
            .headers()
            .get(axum::http::header::CONTENT_TYPE)
            .expect("content-type header")
            .to_str()
            .unwrap()
            .starts_with(prefix));
    }
}

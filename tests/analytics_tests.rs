mod common;

use axum::body::Body;
use axum::http::StatusCode;
use tower::ServiceExt;

async fn setup_with_expenses() -> (axum::Router, String, uuid::Uuid) {
    let app = common::create_test_app();
    let (token, user_id) = common::register_and_login(&app).await;

    let csv_content = include_str!("fixtures/monefy.csv");
    let boundary = "test-boundary-analytics";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"monefy.csv\"\r\n\
         Content-Type: text/csv\r\n\
         \r\n\
         {csv_content}\r\n\
         --{boundary}--\r\n"
    );

    app.clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/expenses/upload")
                .header("authorization", common::auth_header(&token))
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    (app, token, user_id)
}

#[tokio::test]
async fn test_summary() {
    let (app, token, _) = setup_with_expenses().await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/analytics/summary")
                .header("authorization", common::auth_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body["total_income"].is_number());
    assert!(body["total_expense"].is_number());
    assert!(body["net"].is_number());
}

#[tokio::test]
async fn test_by_category() {
    let (app, token, _) = setup_with_expenses().await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/analytics/by-category")
                .header("authorization", common::auth_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body.is_array());
    assert!(body.as_array().unwrap().len() > 0);
}

#[tokio::test]
async fn test_by_account() {
    let (app, token, _) = setup_with_expenses().await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/analytics/by-account")
                .header("authorization", common::auth_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn test_timeline() {
    let (app, token, _) = setup_with_expenses().await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/analytics/timeline?group_by=day")
                .header("authorization", common::auth_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body.is_array());
}

#[tokio::test]
async fn test_analytics_without_auth() {
    let app = common::create_test_app();

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/analytics/summary")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

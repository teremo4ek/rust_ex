mod common;

use axum::body::Body;
use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn test_upload_csv() {
    let app = common::create_test_app();
    let (token, _user_id) = common::register_and_login(&app).await;

    let csv_content = include_str!("fixtures/monefy.csv");

    let boundary = "test-boundary-12345";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"monefy.csv\"\r\n\
         Content-Type: text/csv\r\n\
         \r\n\
         {csv_content}\r\n\
         --{boundary}--\r\n"
    );

    let res = app
        .clone()
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

    assert_eq!(res.status(), StatusCode::CREATED);
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(body["imported"].as_u64().unwrap() > 0);
}

#[tokio::test]
async fn test_upload_without_auth() {
    let app = common::create_test_app();

    let boundary = "test-boundary";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"test.csv\"\r\n\
         Content-Type: text/csv\r\n\
         \r\n\
         date,account,category,amount,currency,converted amount,currency,description\r\n\
         4/22/2026,Cash,Food,-10,BYN,-10,BYN,Test\r\n\
         --{boundary}--\r\n"
    );

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("POST")
                .uri("/api/expenses/upload")
                .header(
                    "content-type",
                    format!("multipart/form-data; boundary={boundary}"),
                )
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn test_list_expenses() {
    let app = common::create_test_app();
    let (token, _user_id) = common::register_and_login(&app).await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/expenses")
                .header("authorization", common::auth_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

#[tokio::test]
async fn test_list_expenses_with_category_filter() {
    let app = common::create_test_app();
    let (token, _user_id) = common::register_and_login(&app).await;

    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri("/api/expenses?category=Food")
                .header("authorization", common::auth_header(&token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(res.status(), StatusCode::OK);
}

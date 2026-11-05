mod common;

use axum::body::Body;
use axum::http::StatusCode;
use tower::ServiceExt;

#[tokio::test]
async fn test_upload_csv() {
    let (app, state) = common::create_test_app_with_state().await;
    common::seed_fixture_rates(&state, rust_decimal::dec!(2.8230)).await;
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
    assert_eq!(body["imported"].as_u64().unwrap(), 41);
    assert_eq!(body["converted"].as_u64().unwrap(), 40);
    assert_eq!(body["skipped"].as_u64().unwrap(), 0);
}

async fn upload_fixture(app: &axum::Router, token: &str) -> serde_json::Value {
    let csv_content = include_str!("fixtures/monefy.csv");
    let boundary = "test-boundary-dedup";
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
                .header("authorization", common::auth_header(token))
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
    serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn test_upload_csv_twice_second_is_all_duplicates() {
    let (app, state) = common::create_test_app_with_state().await;
    common::seed_fixture_rates(&state, rust_decimal::dec!(2.8230)).await;
    let (token, _user_id) = common::register_and_login(&app).await;

    let first = upload_fixture(&app, &token).await;
    assert_eq!(first["imported"].as_u64().unwrap(), 41);
    assert_eq!(first["duplicates"].as_u64().unwrap(), 0);
    assert_eq!(first["converted"].as_u64().unwrap(), 40);

    let second = upload_fixture(&app, &token).await;
    assert_eq!(second["imported"].as_u64().unwrap(), 0);
    assert_eq!(second["duplicates"].as_u64().unwrap(), 41);
    assert_eq!(second["converted"].as_u64().unwrap(), 0);
    assert_eq!(second["skipped"].as_u64().unwrap(), 0);

    let res = app
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
    let body: serde_json::Value = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body["total"].as_u64().unwrap(), 41);
    assert_eq!(body["expenses"].as_array().unwrap().len(), 41);
    assert_eq!(body["expenses"][0]["amount"].as_str().unwrap(), "17.26");
    assert_eq!(body["expenses"][0]["kind"].as_str().unwrap(), "expense");
    assert!(
        body["expenses"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| e["amount"].as_str().is_some_and(|a| !a.starts_with('-')))
    );
    assert!(
        body["expenses"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| matches!(e["kind"].as_str(), Some("expense") | Some("income")))
    );
}

#[tokio::test]
async fn test_upload_without_auth() {
    let app = common::create_test_app().await;

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
    let app = common::create_test_app().await;
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
    let app = common::create_test_app().await;
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

#[tokio::test]
async fn test_upload_pdf_receipt() {
    let (app, state) = common::create_test_app_with_state().await;
    sqlx::query(
        "INSERT OR REPLACE INTO nbrb_rates (date, currency, base_currency, rate)
         VALUES (?, 'USD', 'BYN', '2.8230')",
    )
    .bind(chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap())
    .execute(&state.pool)
    .await
    .unwrap();
    let (token, _user_id) = common::register_and_login(&app).await;

    let pdf = include_bytes!("fixtures/check.pdf");
    let boundary = "test-boundary-pdf";
    let mut body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"check.pdf\"\r\n\
         Content-Type: application/pdf\r\n\
         \r\n"
    )
    .into_bytes();
    body.extend_from_slice(pdf);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

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
    assert_eq!(body["imported"].as_u64().unwrap(), 4);
    assert_eq!(body["duplicates"].as_u64().unwrap(), 0);
    assert_eq!(body["converted"].as_u64().unwrap(), 4);
    assert_eq!(body["skipped"].as_u64().unwrap(), 0);
}

#[tokio::test]
async fn test_upload_corrupt_pdf_returns_bad_request() {
    let app = common::create_test_app().await;
    let (token, _user_id) = common::register_and_login(&app).await;

    let pdf = b"%PDF- definitely corrupt";
    let boundary = "test-boundary-corrupt-pdf";
    let mut body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"corrupt.pdf\"\r\n\
         Content-Type: application/pdf\r\n\
         \r\n"
    )
    .into_bytes();
    body.extend_from_slice(pdf);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let res = app
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

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn test_upload_rejects_non_finite_amount() {
    let app = common::create_test_app().await;
    let (token, _user_id) = common::register_and_login(&app).await;

    let boundary = "test-boundary-nan";
    let body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"nan.csv\"\r\n\
         Content-Type: text/csv\r\n\
         \r\n\
         date,account,category,amount,currency,converted amount,currency,description\r\n\
         4/22/2026,Cash,Food,NaN,BYN,NaN,BYN,Test\r\n\
         --{boundary}--\r\n"
    );

    let res = app
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

    assert_eq!(res.status(), StatusCode::BAD_REQUEST);
}

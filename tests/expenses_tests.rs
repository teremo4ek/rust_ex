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

async fn upload_bytes(
    app: &axum::Router,
    token: &str,
    filename: &str,
    content_type: &str,
    data: &[u8],
) -> serde_json::Value {
    let boundary = "test-boundary-helper";
    let mut body = format!(
        "--{boundary}\r\n\
         Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n\
         Content-Type: {content_type}\r\n\
         \r\n"
    )
    .into_bytes();
    body.extend_from_slice(data);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

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

async fn get_json(app: &axum::Router, token: &str, uri: &str) -> (StatusCode, serde_json::Value) {
    let res = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method("GET")
                .uri(uri)
                .header("authorization", common::auth_header(token))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    let body = serde_json::from_slice(
        &axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap(),
    )
    .unwrap();
    (status, body)
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
    let app = common::create_test_app().await;
    let (token, _user_id) = common::register_and_login(&app).await;
    let pdf = include_bytes!("fixtures/check.pdf");

    let body = upload_bytes(&app, &token, "check.pdf", "application/pdf", pdf).await;

    assert_eq!(body["status"].as_str().unwrap(), "pending");
    assert_eq!(body["date"].as_str().unwrap(), "2026-11-02");
    assert_eq!(body["total"].as_str().unwrap(), "11.51");
    assert_eq!(body["currency"].as_str().unwrap(), "BYN");
    assert_eq!(body["item_count"].as_u64().unwrap(), 4);
    assert!(body["expense_id"].is_null());
    assert_eq!(body["converted"].as_u64().unwrap(), 0);
    assert_eq!(body["skipped"].as_u64().unwrap(), 0);

    let (status, pending) = get_json(&app, &token, "/api/receipts/pending").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(pending["receipts"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn test_upload_pdf_then_csv_attaches_receipt() {
    let (app, state) = common::create_test_app_with_state().await;
    common::seed_rate(
        &state,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
        rust_decimal::dec!(2.8230),
    )
    .await;
    let (token, _user_id) = common::register_and_login(&app).await;
    let pdf = include_bytes!("fixtures/check.pdf");

    upload_bytes(&app, &token, "check.pdf", "application/pdf", pdf).await;

    let csv = "\
date,account,category,amount,currency,converted amount,currency,description
11/2/2026,Payment card,Food,-11.51,BYN,-11.51,BYN,Euroopt
";
    let body = upload_bytes(&app, &token, "match.csv", "text/csv", csv.as_bytes()).await;
    assert_eq!(body["imported"].as_u64().unwrap(), 1);
    assert_eq!(body["receipts_attached"].as_u64().unwrap(), 1);
    assert_eq!(body["converted"].as_u64().unwrap(), 1);

    let (_, list) = get_json(&app, &token, "/api/expenses").await;
    let expense_id = list["expenses"][0]["id"].as_str().unwrap();

    let (status, receipt) = get_json(
        &app,
        &token,
        &format!("/api/expenses/{expense_id}/receipt"),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["total"].as_str().unwrap(), "11.51");
    assert_eq!(receipt["fiscal_id"].as_str().unwrap(), "EDDA8397564658AE0718A4AB");
    let items = receipt["items"].as_array().unwrap();
    assert_eq!(items.len(), 4);
    assert!(items[0].get("receipt_id").is_none());
    assert_eq!(items[0]["name"].as_str().unwrap(), "Яблоко РБ 1кг");
    assert_eq!(items[0]["unit_price"].as_str().unwrap(), "1.16");
    assert_eq!(items[0]["quantity"].as_str().unwrap(), "1.072");

    let (_, pending) = get_json(&app, &token, "/api/receipts/pending").await;
    assert_eq!(pending["receipts"].as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn test_upload_same_pdf_twice_is_duplicate() {
    let app = common::create_test_app().await;
    let (token, _user_id) = common::register_and_login(&app).await;
    let pdf = include_bytes!("fixtures/check.pdf");

    let first = upload_bytes(&app, &token, "check.pdf", "application/pdf", pdf).await;
    assert_eq!(first["status"].as_str().unwrap(), "pending");

    let second = upload_bytes(&app, &token, "check.pdf", "application/pdf", pdf).await;
    assert_eq!(second["status"].as_str().unwrap(), "duplicate");
    assert!(second["expense_id"].is_null());
}

#[tokio::test]
async fn test_get_expense_receipt_without_receipt_is_404() {
    let (app, state) = common::create_test_app_with_state().await;
    common::seed_rate(
        &state,
        chrono::NaiveDate::from_ymd_opt(2026, 11, 2).unwrap(),
        rust_decimal::dec!(2.8230),
    )
    .await;
    let (token, _user_id) = common::register_and_login(&app).await;
    let csv = "\
date,account,category,amount,currency,converted amount,currency,description
11/2/2026,Payment card,Food,-11.51,BYN,-11.51,BYN,Euroopt
";
    upload_bytes(&app, &token, "match.csv", "text/csv", csv.as_bytes()).await;

    let (_, list) = get_json(&app, &token, "/api/expenses").await;
    let expense_id = list["expenses"][0]["id"].as_str().unwrap();

    let (status, _) = get_json(
        &app,
        &token,
        &format!("/api/expenses/{expense_id}/receipt"),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
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

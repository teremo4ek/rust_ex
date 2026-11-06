use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use uuid::Uuid;

use crate::common::error::AppResult;
use crate::common::models::dto::{
    ExpensesQuery, ExpensesResponse, ReceiptUploadResponse, UploadResponse,
};
use crate::common::models::Receipt;
use crate::common::services::expense_service::UploadOutcome;
use crate::common::services::{expense_service, receipt_service};
use crate::common::state::AppState;
use crate::web::middleware::auth::AuthUser;

pub async fn upload(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> AppResult<Response> {
    let (outcome, report) = expense_service::upload(&state, auth.id, multipart).await?;
    let body = match outcome {
        UploadOutcome::Csv(import_report) => Json(UploadResponse {
            imported: import_report.imported,
            duplicates: import_report.duplicates,
            receipts_attached: import_report.receipts_attached,
            converted: report.converted,
            skipped: report.skipped,
        })
        .into_response(),
        UploadOutcome::Receipt(receipt_report) => Json(ReceiptUploadResponse {
            status: receipt_report.status,
            date: receipt_report.date,
            total: receipt_report.total,
            currency: receipt_report.currency,
            item_count: receipt_report.item_count,
            expense_id: receipt_report.expense_id,
            converted: report.converted,
            skipped: report.skipped,
        })
        .into_response(),
    };
    Ok((StatusCode::CREATED, body).into_response())
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ExpensesQuery>,
) -> AppResult<Json<ExpensesResponse>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(100);
    let total = expense_service::count_expenses(&state, auth.id, &query).await?;
    let expenses = expense_service::list_expenses(&state, auth.id, &query).await?;

    Ok(Json(ExpensesResponse {
        expenses,
        total,
        page,
        per_page,
    }))
}

pub async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<StatusCode> {
    expense_service::delete_expense(&state, auth.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn expense_receipt(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Receipt>> {
    let receipt = receipt_service::get_expense_receipt(&state, auth.id, id).await?;
    Ok(Json(receipt))
}

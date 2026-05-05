use axum::extract::{Multipart, Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use uuid::Uuid;

use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::dto::{ExpensesQuery, ExpensesResponse, UploadResponse};
use crate::services::expense_service;
use crate::state::AppState;

pub async fn upload(
    State(state): State<AppState>,
    auth: AuthUser,
    multipart: Multipart,
) -> AppResult<(StatusCode, Json<UploadResponse>)> {
    let imported = expense_service::upload_csv(&state, auth.id, multipart).await?;
    Ok((StatusCode::CREATED, Json(UploadResponse { imported })))
}

pub async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<ExpensesQuery>,
) -> AppResult<Json<ExpensesResponse>> {
    let page = query.page.unwrap_or(1).max(1);
    let per_page = query.per_page.unwrap_or(50).min(100);
    let total = expense_service::count_expenses(&state, auth.id, &query).await;
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

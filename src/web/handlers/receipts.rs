use axum::extract::State;
use axum::response::Response;
use axum::Json;

use crate::common::error::{AppError, AppResult};
use crate::common::models::dto::{PendingReceiptsResponse, ReceiptUrlRequest};
use crate::common::services::{expense_service, receipt_service};
use crate::common::state::AppState;
use crate::web::handlers::expenses::outcome_response;
use crate::web::middleware::auth::AuthUser;

pub async fn pending(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<PendingReceiptsResponse>> {
    let receipts = receipt_service::list_pending_receipts(&state, auth.id).await?;
    Ok(Json(PendingReceiptsResponse { receipts }))
}

pub async fn import_url(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<ReceiptUrlRequest>,
) -> AppResult<Response> {
    let url = req.url.trim();
    if url.is_empty() {
        return Err(AppError::BadRequest("url must not be empty".into()));
    }
    let (outcome, conversion) = expense_service::import_url(&state, auth.id, url).await?;
    Ok(outcome_response(outcome, conversion))
}

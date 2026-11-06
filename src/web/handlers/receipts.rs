use axum::extract::State;
use axum::Json;

use crate::common::error::AppResult;
use crate::common::models::dto::PendingReceiptsResponse;
use crate::common::services::receipt_service;
use crate::common::state::AppState;
use crate::web::middleware::auth::AuthUser;

pub async fn pending(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AppResult<Json<PendingReceiptsResponse>> {
    let receipts = receipt_service::list_pending_receipts(&state, auth.id).await?;
    Ok(Json(PendingReceiptsResponse { receipts }))
}

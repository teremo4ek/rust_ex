use axum::extract::State;
use axum::Json;
use validator::Validate;

use crate::common::error::AppResult;
use crate::common::models::{LoginRequest, LoginResponse};
use crate::common::services::auth_service;
use crate::common::state::AppState;

pub async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> AppResult<Json<LoginResponse>> {
    req.validate()?;

    let (token, user) = auth_service::login(
        &state,
        &req.email,
        &req.password,
        &state.jwt_secret,
        state.jwt_expires_in,
    )
    .await?;

    Ok(Json(LoginResponse {
        token,
        user: user.public(),
    }))
}

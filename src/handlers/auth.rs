use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use validator::Validate;

use crate::error::AppResult;
use crate::models::{LoginRequest, LoginResponse, RegisterRequest};
use crate::services::auth_service;
use crate::state::AppState;

pub async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> AppResult<(StatusCode, Json<LoginResponse>)> {
    req.validate()?;

    let (token, user) = auth_service::register(
        &state,
        req.email,
        req.password,
        &state.jwt_secret,
        state.jwt_expires_in,
    )
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(LoginResponse {
            token,
            user: user.public(),
        }),
    ))
}

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

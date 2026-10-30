use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use uuid::Uuid;

use crate::common::error::AppError;
use crate::common::services::auth_service;
use crate::common::state::AppState;

#[derive(Debug, Clone)]
pub struct AuthUser {
    pub id: Uuid,
    #[allow(dead_code)]
    pub email: String,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = AppError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let header = parts
            .headers
            .get("Authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or(AppError::Unauthorized)?;

        let token = header
            .strip_prefix("Bearer ")
            .ok_or(AppError::Unauthorized)?;

        let claims = auth_service::validate_token(token, &state.jwt_secret)?;
        let user_id: Uuid = claims
            .sub
            .parse()
            .map_err(|_| AppError::Unauthorized)?;

        let user = auth_service::find_by_id(state, user_id)
            .await?
            .ok_or(AppError::Unauthorized)?;

        Ok(AuthUser {
            id: user.id,
            email: user.email,
        })
    }
}

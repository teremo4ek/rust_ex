use axum::extract::{Query, State};
use axum::Json;

use crate::error::AppResult;
use crate::middleware::auth::AuthUser;
use crate::models::dto::AnalyticsQuery;
use crate::services::{analytics_service, expense_service};
use crate::state::AppState;

pub async fn summary(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let expenses =
        expense_service::get_user_expenses(&state, auth.id, query.from, query.to).await;
    Ok(Json(serde_json::to_value(analytics_service::summary(&expenses)).unwrap()))
}

pub async fn by_category(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let expenses =
        expense_service::get_user_expenses(&state, auth.id, query.from, query.to).await;
    Ok(Json(serde_json::to_value(analytics_service::by_category(&expenses)).unwrap()))
}

pub async fn by_account(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let expenses =
        expense_service::get_user_expenses(&state, auth.id, query.from, query.to).await;
    Ok(Json(serde_json::to_value(analytics_service::by_account(&expenses)).unwrap()))
}

pub async fn timeline(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(query): Query<AnalyticsQuery>,
) -> AppResult<Json<serde_json::Value>> {
    let expenses =
        expense_service::get_user_expenses(&state, auth.id, query.from, query.to).await;
    let group_by = query.group_by.as_deref().unwrap_or("day");
    Ok(Json(serde_json::to_value(analytics_service::timeline(&expenses, group_by)).unwrap()))
}

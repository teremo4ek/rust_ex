use axum::routing::{delete, get, post};
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use crate::handlers;
use crate::state::AppState;

pub fn create_router(state: AppState) -> Router {
    let protected_routes = Router::new()
        .route("/api/expenses/upload", post(handlers::expenses::upload))
        .route("/api/expenses", get(handlers::expenses::list))
        .route("/api/expenses/{id}", delete(handlers::expenses::delete))
        .route("/api/analytics/summary", get(handlers::analytics::summary))
        .route("/api/analytics/by-category", get(handlers::analytics::by_category))
        .route("/api/analytics/by-account", get(handlers::analytics::by_account))
        .route("/api/analytics/timeline", get(handlers::analytics::timeline));

    Router::new()
        .route("/api/auth/register", post(handlers::auth::register))
        .route("/api/auth/login", post(handlers::auth::login))
        .merge(protected_routes)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
}

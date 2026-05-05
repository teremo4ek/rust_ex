use axum::routing::get;
use expense_tracker::config::Config;
use expense_tracker::routes;
use expense_tracker::state::AppState;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = Config::from_env();
    let state = AppState::from_config(&config);

    let app = routes::create_router(state)
        .route("/health", get(|| async { "ok" }));

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

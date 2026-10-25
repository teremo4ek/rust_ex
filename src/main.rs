#[cfg(debug_assertions)]
mod debug_setup;

use axum::routing::get;
use expense_tracker::cli;
use expense_tracker::common::config::Config;
use expense_tracker::common::state::AppState;
use expense_tracker::web::routes;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() {
    match std::env::args().nth(1).as_deref() {
        None => run_server().await,
        Some("cli") => cli::run().await,
        Some(_) => {
            eprintln!("Unknown argument. Usage: expense_tracker [cli]");
            std::process::exit(2);
        }
    }
}

async fn run_server() {
    dotenvy::dotenv().ok();
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let mut config = Config::from_env();
    #[cfg(debug_assertions)]
    {
        config.jwt_secret = debug_setup::DEBUG_JWT_SECRET.into();
    }
    let state = AppState::from_config(&config);

    #[cfg(debug_assertions)]
    debug_setup::seed_debug_user(&state).await;

    let app = routes::create_router(state)
        .route("/health", get(|| async { "ok" }));

    let addr = format!("0.0.0.0:{}", config.port);
    tracing::info!("server starting on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

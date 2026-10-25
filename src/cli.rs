mod commands;
mod session;
mod views;

use std::io;
use uuid::Uuid;

use crate::common::config::Config;
#[cfg(debug_assertions)]
use crate::common::debug_setup;
use crate::common::state::AppState;

pub async fn run() {
    let state = AppState::from_config(&Config {
        // token is discarded within the session; the secret never leaves the process
        jwt_secret: Uuid::new_v4().to_string(),
        jwt_expires_in: 3600,
        port: 0,
    });
    #[cfg(debug_assertions)]
    debug_setup::seed_debug_user(&state).await;
    let mut input = io::stdin().lock();
    let mut output = io::stdout();
    if let Err(e) = session::run(&mut input, &mut output, state).await {
        eprintln!("{e}");
    }
}

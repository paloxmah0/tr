use axum::{
    routing::{get, post},
    Router,
};

use crate::state::AppState;

pub mod accounts;
pub mod analytics;
pub mod backtest;
pub mod notes;
pub mod signals;
pub mod strategies;
pub mod trades;

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(landing))
        .route("/health", get(health))
        // accounts
        .route("/accounts", post(accounts::create).get(accounts::list))
        .route("/accounts/:id", get(accounts::get))
        .route("/accounts/:id/mode", post(accounts::set_mode))
        // strategies (manual + CRUD)
        .route("/accounts/:id/strategies", post(strategies::create).get(strategies::list))
        .route("/strategies/:id", get(strategies::get).put(strategies::update).delete(strategies::delete))
        // backtest a strategy against historical data
        .route("/strategies/:id/backtest", post(backtest::run))
        // notes (ingestion)
        .route("/accounts/:id/notes", post(notes::create).get(notes::list))
        .route("/notes/:id", get(notes::get).post(notes::process))
        // signals & trades
        .route("/accounts/:id/signals", get(signals::list))
        .route("/accounts/:id/trades", get(trades::list))
        .route("/trades/:id/close", post(trades::close))
        // analytics & insights
        .route("/accounts/:id/analytics", get(analytics::summary))
        .route("/accounts/:id/insights", get(analytics::insights))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn landing() -> axum::response::Html<&'static str> {
    axum::response::Html(LANDING_HTML)
}

const LANDING_HTML: &str = include_str!("landing.html");

mod analytics;
mod api;
mod backtest;
mod config;
mod db;
mod domain;
mod engine;
mod engine_loop;
mod error;
mod execution;
mod ingest;
mod insights;
mod llm;
mod market;
mod state;

use std::sync::Arc;

use config::Settings;
use domain::AssetClass;
use llm::LlmClient;
use market::{Broker, DerivClient, MarketProvider, MarketRegistry, OandaClient, RestProvider};
use sqlx::postgres::PgPoolOptions;
use state::AppState;

use crate::db::Db;
use crate::ingest::Ingestor;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "info,sqlx=warn,hyper=warn".into()),
        )
        .init();

    let settings = Arc::new(Settings::load()?);

    // Use a lazy pool so the server starts even if PostgreSQL isn't reachable.
    // Migrations + DB-dependent endpoints will fail gracefully until a DB is up.
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect_lazy(&settings.database.url)?;

    let db = Db::new(pool);
    match db.run_migrations().await {
        Ok(()) => tracing::info!("database migrations applied"),
        Err(e) => tracing::warn!(error = %e, "database unavailable — server starting in degraded mode (DB endpoints will return 500)"),
    }

    let llm = Arc::new(LlmClient::new(&settings.llm));
    let ingest = Arc::new(Ingestor::new(db.clone(), llm.clone()));

    // Forex: prefer OANDA (spot data + live broker) when a token is configured,
    // otherwise fall back to the generic REST adapter.
    let (forex, forex_broker): (Arc<dyn MarketProvider>, Option<Arc<dyn Broker>>) =
        if settings.oanda.api_key.is_empty() {
            (Arc::new(RestProvider::new(&settings.forex, AssetClass::Forex)), None)
        } else {
            let oanda = OandaClient::new(&settings.oanda, settings.deriv_granularity_secs);
            let broker: Arc<dyn Broker> = oanda.clone();
            (oanda, Some(broker))
        };

    // Derivative indices (and forex fallback) via the real Deriv WebSocket client.
    let deriv_client = Arc::new(DerivClient::new(
        &settings.deriv.app_id,
        &settings.deriv.api_key,
        settings.deriv_granularity_secs,
    ));
    let deriv: Arc<dyn MarketProvider> = deriv_client.clone();
    let deriv_broker: Option<Arc<dyn Broker>> = if settings.deriv.api_key.is_empty() {
        tracing::warn!("DERIV_PROVIDER_API_TOKEN not set; deriv live mode will fall back to simulated fills");
        None
    } else {
        Some(deriv_client.clone())
    };

    let markets = Arc::new(MarketRegistry::new(forex, deriv, forex_broker, deriv_broker));

    let state = AppState {
        settings: settings.clone(),
        db: db.clone(),
        llm: llm.clone(),
        ingest: ingest.clone(),
        markets: markets.clone(),
    };

    // Background engine loop.
    {
        let db = db.clone();
        let markets = markets.clone();
        let tick = settings.engine_tick_secs;
        tokio::spawn(async move { engine_loop::run(db, markets, tick).await });
    }

    let app = api::router(state);
    let addr = format!("{}:{}", settings.server.host, settings.server.port);
    tracing::info!("listening on {addr}");
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;
    Ok(())
}

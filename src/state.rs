use crate::config::Settings;
use crate::db::Db;
use crate::dynamic_config::DynamicConfig;
use crate::ingest::Ingestor;
use crate::llm::LlmClient;
use crate::market::MarketRegistry;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub settings: Arc<Settings>,
    pub db: Db,
    pub config: Arc<DynamicConfig>,
    pub llm: Arc<LlmClient>,
    pub ingest: Arc<Ingestor>,
    pub markets: Arc<MarketRegistry>,
}

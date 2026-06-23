use crate::domain::{Side, TradingMode};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Emitted by the strategy engine when rules evaluate to a tradeable condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub id: Uuid,
    pub strategy_id: Uuid,
    pub account_id: Uuid,
    pub symbol: String,
    pub side: Side,
    pub price: Decimal,
    pub strength: Decimal, // 0.0-1.0 aggregate of rule weights
    pub rationale: String,
    pub mode: TradingMode,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct SignalFilter {
    pub strategy_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub limit: Option<i64>,
}

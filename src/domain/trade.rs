use crate::domain::{OrderType, Side, TradingMode};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "trade_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TradeStatus {
    Open,
    Closed,
    Rejected,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: Uuid,
    pub account_id: Uuid,
    pub strategy_id: Uuid,
    pub signal_id: Option<Uuid>,
    pub symbol: String,
    pub side: Side,
    pub order_type: OrderType,
    pub mode: TradingMode,
    pub size: Decimal,
    pub entry_price: Decimal,
    pub exit_price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub pnl: Option<Decimal>,
    pub status: TradeStatus,
    pub opened_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CloseTrade {
    pub exit_price: Decimal,
}

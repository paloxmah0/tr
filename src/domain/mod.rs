use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub mod note;
pub mod signal;
pub mod strategy;
pub mod trade;

pub use strategy::*;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "trading_mode", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum TradingMode {
    Paper,
    Signals,
    Live,
}

impl TradingMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "paper" => Some(Self::Paper),
            "signals" => Some(Self::Signals),
            "live" => Some(Self::Live),
            _ => None,
        }
    }
}

impl Default for TradingMode {
    fn default() -> Self { Self::Paper }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "side", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "asset_class", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum AssetClass {
    Forex,
    DerivIndex,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, sqlx::Type, PartialEq, Eq)]
#[sqlx(type_name = "order_type", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum OrderType {
    Market,
    Limit,
    Stop,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Account {
    pub id: Uuid,
    pub label: String,
    pub broker: String,
    pub account_ref: String,
    pub mode: TradingMode,
    pub balance: Decimal,
    pub currency: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candle {
    pub symbol: String,
    pub ts: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}

use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// A single, deterministic trading rule. Strategies compose one or more rules.
/// The `expr` field is a small rule DSL evaluated by the engine (see engine/rules.rs).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub id: uuid::Uuid,
    pub strategy_id: uuid::Uuid,
    pub name: String,
    /// DSL expression, e.g. `rsi(14) < 30 and price > ema(50)`
    pub expr: String,
    pub weight: Decimal,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Strategy {
    pub id: uuid::Uuid,
    pub account_id: uuid::Uuid,
    pub name: String,
    pub description: Option<String>,
    pub asset_class: crate::domain::AssetClass,
    pub symbols: Vec<String>,
    /// Risk parameters in pips (for forex) or points (for indices).
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    /// Fraction of balance to risk per trade, 0.0-1.0
    pub risk_per_trade: Decimal,
    pub enabled: bool,
    pub source: StrategySource,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, sqlx::Type)]
#[sqlx(type_name = "strategy_source", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum StrategySource {
    Manual,
    Llm,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateStrategy {
    pub name: String,
    pub description: Option<String>,
    pub asset_class: crate::domain::AssetClass,
    pub symbols: Vec<String>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    #[serde(default = "default_risk")]
    pub risk_per_trade: Decimal,
    pub rules: Vec<CreateRule>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CreateRule {
    pub name: String,
    pub expr: String,
    #[serde(default = "one")]
    pub weight: Decimal,
}

#[derive(Debug, Clone, Deserialize)]
pub struct UpdateStrategy {
    pub name: Option<String>,
    pub description: Option<String>,
    pub symbols: Option<Vec<String>>,
    pub stop_loss: Option<Option<Decimal>>,
    pub take_profit: Option<Option<Decimal>>,
    pub risk_per_trade: Option<Decimal>,
    pub enabled: Option<bool>,
}

fn default_risk() -> Decimal { Decimal::new(1, 2) } // 0.01
fn one() -> Decimal { Decimal::ONE }

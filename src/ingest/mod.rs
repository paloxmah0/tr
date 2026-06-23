use crate::db::Db;
use crate::domain::strategy::{CreateRule, CreateStrategy};
use crate::domain::{AssetClass, StrategySource};
use crate::error::{AppError, AppResult};
use crate::llm::LlmClient;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

/// Extracted strategy schema returned by the LLM.
#[derive(Debug, Deserialize)]
struct ExtractedStrategy {
    name: String,
    #[serde(default)]
    description: Option<String>,
    asset_class: String,
    symbols: Vec<String>,
    #[serde(default)]
    stop_loss: Option<f64>,
    #[serde(default)]
    take_profit: Option<f64>,
    #[serde(default)]
    risk_per_trade: Option<f64>,
    rules: Vec<ExtractedRule>,
}

#[derive(Debug, Deserialize)]
struct ExtractedRule {
    name: String,
    expr: String,
    #[serde(default)]
    weight: Option<f64>,
}

const SYSTEM: &str = r#"You convert trading notes into a machine-executable strategy.

The rule DSL supports these functions and operators:
- Indicators: rsi(period), ema(period), sma(period), macd(), atr(period), price, high, low, close, open, volume
- Comparators: <, <=, >, >=, ==, !=
- Logic: and, or, not
- Arithmetic: +, -, *, /
- Functions: cross(a, b), crossup(a, b), crossdown(a, b), pct_change(periods)

Examples:
- "buy when RSI is oversold": rsi(14) < 30
- "price above 50 EMA and MACD positive": price > ema(50) and macd() > 0
- "golden cross": crossup(ema(50), ema(200))

Each rule must produce a boolean. Combine entry rules with 'and'/'or'. For exits, separate rules referencing stop_loss/take_profit are added automatically from the strategy's SL/TP.

Respond ONLY with a JSON object:
{
  "name": "...",
  "description": "...",
  "asset_class": "forex" | "derivindex",
  "symbols": ["EUR/USD", ...],
  "stop_loss": 30.0,      // pips or points, null if none
  "take_profit": 60.0,
  "risk_per_trade": 0.01, // 0..1
  "rules": [
    { "name": "rsi_oversold", "expr": "rsi(14) < 30", "weight": 1.0 }
  ]
}"#;

pub struct Ingestor {
    pub db: Db,
    pub llm: std::sync::Arc<LlmClient>,
}

impl Ingestor {
    pub fn new(db: Db, llm: std::sync::Arc<LlmClient>) -> Self {
        Self { db, llm }
    }

    /// Extract a strategy from a note's content and persist it for an account.
    pub async fn process_note(&self, note_id: Uuid, account_id: Uuid) -> AppResult<Uuid> {
        let note = self
            .db
            .get_note(note_id)
            .await?
            .ok_or_else(|| AppError::NotFound("note".into()))?;

        let user_msg = format!(
            "Convert the following trading notes into a strategy JSON.\n\n\
             Note title: {}\n\
             Content type: {}\n\n\
             Content:\n{}\n\n\
             Return the strategy JSON object only.",
            note.title, note.content_type, note.content
        );

        let extracted = match self.llm.extract_json(SYSTEM, &user_msg).await {
            Ok(v) => v,
            Err(e) => {
                self.db.mark_note_failed(note_id, &e.to_string()).await?;
                return Err(e);
            }
        };

        let parsed: ExtractedStrategy = match serde_json::from_value(extracted) {
            Ok(p) => p,
            Err(e) => {
                self.db.mark_note_failed(note_id, &format!("schema: {e}")).await?;
                return Err(AppError::Llm(format!("extraction schema error: {e}")));
            }
        };

        let asset_class = match parsed.asset_class.to_ascii_lowercase().as_str() {
            "forex" | "fx" => AssetClass::Forex,
            _ => AssetClass::DerivIndex,
        };

        let to_dec = |f: Option<f64>| -> Option<Decimal> {
            f.and_then(|x| Decimal::try_from(x).ok())
        };

        let req = CreateStrategy {
            name: parsed.name,
            description: parsed.description,
            asset_class,
            symbols: parsed.symbols,
            stop_loss: to_dec(parsed.stop_loss),
            take_profit: to_dec(parsed.take_profit),
            risk_per_trade: to_dec(parsed.risk_per_trade).unwrap_or_else(|| Decimal::new(1, 2)),
            rules: parsed
                .rules
                .into_iter()
                .map(|r| CreateRule {
                    name: r.name,
                    expr: r.expr,
                    weight: to_dec(r.weight).unwrap_or(Decimal::ONE),
                })
                .collect(),
        };

        let strategy = self
            .db
            .create_strategy(account_id, &req, StrategySource::Llm)
            .await?;
        self.db.mark_note_extracted(note_id).await?;
        Ok(strategy.id)
    }
}

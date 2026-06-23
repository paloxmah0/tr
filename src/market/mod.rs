use crate::config::ProviderSettings;
use crate::domain::{AssetClass, Candle, Side};
use crate::error::{AppError, AppResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::sync::Arc;

pub mod deriv;
pub mod oanda;

pub use deriv::DerivClient;
pub use oanda::OandaClient;

#[async_trait]
pub trait MarketProvider: Send + Sync {
    async fn candles(&self, symbol: &str, count: usize) -> AppResult<Vec<Candle>>;
    async fn quote(&self, symbol: &str) -> AppResult<Quote>;
    fn asset_class(&self) -> AssetClass;
}

#[derive(Debug, Clone)]
pub struct Quote {
    pub symbol: String,
    pub bid: Decimal,
    pub ask: Decimal,
    pub ts: DateTime<Utc>,
}

/// A live order placement interface. Paper/signals modes don't use this;
/// only `TradingMode::Live` routes through a `Broker`.
#[async_trait]
pub trait Broker: Send + Sync {
    /// Place an order. Brokers interpret the fields they support:
    /// - Deriv uses `stake`, `duration_secs`, and monetary `stop_loss_amount`/`take_profit_amount`.
    /// - OANDA (spot) converts `stake` to units and uses price-based SL/TP, ignoring duration.
    async fn place_order(&self, req: OrderRequest) -> AppResult<BrokerOrder>;
    async fn balance(&self) -> AppResult<Decimal>;
    /// Human label for logs/diagnostics.
    fn name(&self) -> &'static str;
}

/// Unified order request covering both contract (Deriv) and spot (OANDA) brokers.
#[derive(Debug, Clone)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: Side,
    /// Notional in account currency (Deriv stake; OANDA converts to units).
    pub stake: Decimal,
    /// Contract duration in seconds (Deriv). Ignored by spot brokers.
    pub duration_secs: Option<u32>,
    /// Price-based stop-loss (spot brokers).
    pub stop_loss_price: Option<Decimal>,
    /// Price-based take-profit (spot brokers).
    pub take_profit_price: Option<Decimal>,
    /// Monetary stop-loss (Deriv limit orders).
    pub stop_loss_amount: Option<Decimal>,
    /// Monetary take-profit (Deriv limit orders).
    pub take_profit_amount: Option<Decimal>,
}

#[derive(Debug, Clone)]
pub struct BrokerOrder {
    pub broker_ref: String,
    pub filled_price: Decimal,
    pub balance_after: Decimal,
}

/// A REST-based provider that expects an OpenAPI-style OHLCV endpoint:
///   GET {base}/candles?symbol=..&limit=..  -> [{ ts, o, h, l, c, v }]
///   GET {base}/quote?symbol=..             -> { bid, ask, ts }
/// Subclass per asset class via constructor. Override methods in real providers.
pub struct RestProvider {
    client: reqwest::Client,
    base_url: String,
    api_key: String,
    account_id: String,
    class: AssetClass,
}

impl RestProvider {
    pub fn new(settings: &ProviderSettings, class: AssetClass) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: settings.base_url.trim_end_matches('/').to_string(),
            api_key: settings.api_key.clone(),
            account_id: settings.account_id.clone(),
            class,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CandlesResp {
    #[serde(default)]
    data: Option<Vec<CandleDto>>,
}

#[derive(Debug, Deserialize)]
struct CandleDto {
    ts: DateTime<Utc>,
    o: Decimal,
    h: Decimal,
    l: Decimal,
    c: Decimal,
    #[serde(default)]
    v: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
struct QuoteDto {
    bid: Decimal,
    ask: Decimal,
    #[serde(default)]
    ts: Option<DateTime<Utc>>,
}

#[async_trait]
impl MarketProvider for RestProvider {
    fn asset_class(&self) -> AssetClass { self.class }

    async fn candles(&self, symbol: &str, count: usize) -> AppResult<Vec<Candle>> {
        let url = format!("{}/candles", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .query(&[("symbol", symbol), ("limit", &count.to_string())])
            .send()
            .await
            .map_err(|e| AppError::Market(e.to_string()))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(AppError::Market(format!("candles HTTP {s}: {b}")));
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| AppError::Market(e.to_string()))?;

        // Accept either { data: [...] } or bare array.
        let arr = if let Some(d) = body.get("data").and_then(|v| v.as_array()) {
            d.clone()
        } else if let Some(a) = body.as_array() {
            a.clone()
        } else {
            return Err(AppError::Market("unexpected candles payload".into()));
        };

        let mut out = Vec::with_capacity(arr.len());
        for v in arr {
            let o = field_dec(&v, &["o", "open"])?;
            let h = field_dec(&v, &["h", "high"])?;
            let l = field_dec(&v, &["l", "low"])?;
            let c = field_dec(&v, &["c", "close"])?;
            let vol = field_dec(&v, &["v", "volume"]).unwrap_or(Decimal::ZERO);
            let ts = field_ts(&v)?;
            out.push(Candle { symbol: symbol.to_string(), ts, open: o, high: h, low: l, close: c, volume: vol });
        }
        Ok(out)
    }

    async fn quote(&self, symbol: &str) -> AppResult<Quote> {
        let url = format!("{}/quote", self.base_url);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(&self.api_key)
            .query(&[("symbol", symbol)])
            .send()
            .await
            .map_err(|e| AppError::Market(e.to_string()))?;

        if !resp.status().is_success() {
            let s = resp.status();
            let b = resp.text().await.unwrap_or_default();
            return Err(AppError::Market(format!("quote HTTP {s}: {b}")));
        }
        let body: serde_json::Value = resp.json().await.map_err(|e| AppError::Market(e.to_string()))?;
        let bid = field_dec(&body, &["bid"])?;
        let ask = field_dec(&body, &["ask"])?;
        let ts = field_ts(&body).unwrap_or_else(|_| Utc::now());
        Ok(Quote { symbol: symbol.to_string(), bid, ask, ts })
    }
}

fn field_dec(v: &serde_json::Value, keys: &[&str]) -> AppResult<Decimal> {
    for k in keys {
        if let Some(val) = v.get(*k) {
            return decimal_from(val);
        }
    }
    Err(AppError::Market(format!("missing field among {keys:?}")))
}

fn decimal_from(v: &serde_json::Value) -> AppResult<Decimal> {
    match v {
        serde_json::Value::Number(n) => {
            if let Some(f) = n.as_f64() {
                return Decimal::try_from(f).map_err(|e| AppError::Market(e.to_string()));
            }
            if let Some(i) = n.as_i64() {
                return Ok(Decimal::from(i));
            }
            Err(AppError::Market("unsupported number".into()))
        }
        serde_json::Value::String(s) => {
            Decimal::from_str_exact(s).map_err(|e| AppError::Market(e.to_string()))
        }
        _ => Err(AppError::Market("expected number".into())),
    }
}

fn field_ts(v: &serde_json::Value) -> AppResult<DateTime<Utc>> {
    for k in &["ts", "time", "timestamp", "t"] {
        if let Some(val) = v.get(*k) {
            if let Some(s) = val.as_str() {
                return DateTime::parse_from_rfc3339(s)
                    .map(|d| d.with_timezone(&Utc))
                    .map_err(|e| AppError::Market(e.to_string()));
            }
            if let Some(i) = val.as_i64() {
                return chrono::DateTime::from_timestamp(i, 0)
                    .ok_or_else(|| AppError::Market("bad timestamp".into()));
            }
        }
    }
    Err(AppError::Market("missing timestamp".into()))
}

/// A registry mapping asset class -> provider, chosen per strategy.
#[derive(Clone)]
pub struct MarketRegistry {
    pub forex: Arc<dyn MarketProvider>,
    pub deriv: Arc<dyn MarketProvider>,
    /// Live broker for forex spot (OANDA) — used in live mode.
    pub forex_broker: Option<Arc<dyn Broker>>,
    /// Live broker for derivative indices (Deriv) — used in live mode.
    pub deriv_broker: Option<Arc<dyn Broker>>,
}

impl MarketRegistry {
    pub fn new(
        forex: Arc<dyn MarketProvider>,
        deriv: Arc<dyn MarketProvider>,
        forex_broker: Option<Arc<dyn Broker>>,
        deriv_broker: Option<Arc<dyn Broker>>,
    ) -> Self {
        Self { forex, deriv, forex_broker, deriv_broker }
    }

    pub fn select(&self, class: AssetClass) -> &Arc<dyn MarketProvider> {
        match class {
            AssetClass::Forex => &self.forex,
            AssetClass::DerivIndex => &self.deriv,
        }
    }

    /// Pick the live broker for an asset class, if configured.
    pub fn broker_for(&self, class: AssetClass) -> Option<&Arc<dyn Broker>> {
        match class {
            AssetClass::Forex => self.forex_broker.as_ref(),
            AssetClass::DerivIndex => self.deriv_broker.as_ref(),
        }
    }
}

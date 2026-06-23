//! OANDA v20 REST adapter — spot forex market data + live order placement with
//! real stop-loss / take-profit price levels (unlike Deriv's duration contracts).
//!
//! Implements both `MarketProvider` (candles + pricing) and `Broker` (market
//! orders with `stopLossOnFill` / `takeProfitOnFill`). Used for the forex asset
//! class when `OANDA_PROVIDER_API_TOKEN` is configured.

use crate::config::ProviderSettings;
use crate::domain::{AssetClass, Candle, Side};
use crate::error::{AppError, AppResult};
use crate::market::{Broker, BrokerOrder, MarketProvider, OrderRequest, Quote};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::sync::Arc;

pub struct OandaClient {
    client: reqwest::Client,
    base_url: String,
    token: String,
    account_id: String,
    granularity: String,
}

impl OandaClient {
    pub fn new(settings: &ProviderSettings, granularity_secs: u32) -> Arc<Self> {
        Arc::new(Self {
            client: reqwest::Client::new(),
            base_url: settings.base_url.trim_end_matches('/').to_string(),
            token: settings.api_key.clone(),
            account_id: settings.account_id.clone(),
            granularity: granularity_to_oanda(granularity_secs),
        })
    }

    fn auth(&self) -> reqwest::header::HeaderValue {
        reqwest::header::HeaderValue::from_str(&format!("Bearer {}", self.token))
            .expect("invalid oanda token chars")
    }

    async fn get_json(&self, path: &str, query: &[(&str, &str)]) -> AppResult<serde_json::Value> {
        let resp = self
            .client
            .get(format!("{}{}", self.base_url, path))
            .header("Authorization", self.auth())
            .header("Content-Type", "application/json")
            .query(query)
            .send()
            .await
            .map_err(|e| AppError::Market(format!("oanda: {e}")))?;
        self.handle(resp, "GET").await
    }

    async fn post_json(&self, path: &str, body: &serde_json::Value) -> AppResult<serde_json::Value> {
        let resp = self
            .client
            .post(format!("{}{}", self.base_url, path))
            .header("Authorization", self.auth())
            .header("Content-Type", "application/json")
            .json(body)
            .send()
            .await
            .map_err(|e| AppError::Market(format!("oanda: {e}")))?;
        self.handle(resp, "POST").await
    }

    async fn handle(&self, resp: reqwest::Response, op: &str) -> AppResult<serde_json::Value> {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        if !status.is_success() {
            return Err(AppError::Market(format!("oanda {op} HTTP {status}: {text}")));
        }
        serde_json::from_str::<serde_json::Value>(&text)
            .map_err(|e| AppError::Market(format!("oanda {op} json: {e}")))
    }
}

/// Map "EUR/USD" or "EURUSD" to OANDA's "EUR_USD" instrument format.
pub fn to_oanda_instrument(symbol: &str) -> String {
    let s: String = symbol.chars().filter(|c| c.is_alphanumeric()).collect();
    if s.len() == 6 {
        format!("{}_{}", &s[..3], &s[3..])
    } else {
        s
    }
}

/// Map a granularity in seconds to the nearest OANDA granularity code.
fn granularity_to_oanda(secs: u32) -> String {
    const TABLE: &[(u32, &str)] = &[
        (5, "S5"), (10, "S10"), (15, "S15"), (30, "S30"),
        (60, "M1"), (120, "M2"), (300, "M5"), (900, "M15"),
        (1800, "M30"), (3600, "H1"), (7200, "H2"), (14400, "H4"),
        (28800, "H8"), (43200, "H12"), (86400, "D"),
    ];
    let mut best = "M1";
    let mut best_diff = u64::MAX;
    for (s, code) in TABLE {
        let d = (*s as i64 - secs as i64).unsigned_abs();
        if d < best_diff {
            best_diff = d;
            best = code;
        }
    }
    best.to_string()
}

/// Parse a Decimal from a JSON value (accepts string or number).
fn dec(v: &serde_json::Value) -> Option<Decimal> {
    match v {
        serde_json::Value::String(s) => Decimal::from_str_exact(s).ok(),
        serde_json::Value::Number(n) => n.as_f64().and_then(|f| Decimal::try_from(f).ok()),
        _ => None,
    }
}
/// Convenience: parse from an Option<&Value> (i.e. `.get(...)` result).
fn dec_opt(v: Option<&serde_json::Value>) -> Option<Decimal> {
    v.and_then(dec)
}

#[async_trait]
impl MarketProvider for OandaClient {
    fn asset_class(&self) -> AssetClass { AssetClass::Forex }

    async fn candles(&self, symbol: &str, count: usize) -> AppResult<Vec<Candle>> {
        let inst = to_oanda_instrument(symbol);
        let count = count.clamp(1, 5000).to_string();
        let body = self
            .get_json(
                &format!("/v3/instruments/{inst}/candles"),
                &[("price", "M"), ("granularity", &self.granularity), ("count", &count)],
            )
            .await?;
        let Some(candles) = body.get("candles").and_then(|c| c.as_array()) else {
            return Err(AppError::Market("oanda: no candles".into()));
        };
        let mut out = Vec::with_capacity(candles.len());
        for c in candles {
            if c.get("complete").and_then(|v| v.as_bool()) == Some(false) {
                continue; // skip the in-progress candle
            }
            let ts = c
                .get("time")
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|d| d.with_timezone(&Utc))
                .unwrap_or_else(Utc::now);
            let mid = c.get("mid").ok_or_else(|| AppError::Market("oanda: no mid".into()))?;
            let open = dec_opt(mid.get("o")).ok_or_else(|| AppError::Market("oanda: bad o".into()))?;
            let high = dec_opt(mid.get("h")).ok_or_else(|| AppError::Market("oanda: bad h".into()))?;
            let low = dec_opt(mid.get("l")).ok_or_else(|| AppError::Market("oanda: bad l".into()))?;
            let close = dec_opt(mid.get("c")).ok_or_else(|| AppError::Market("oanda: bad c".into()))?;
            let volume = dec_opt(c.get("volume")).unwrap_or(Decimal::ZERO);
            out.push(Candle { symbol: symbol.to_string(), ts, open, high, low, close, volume });
        }
        Ok(out)
    }

    async fn quote(&self, symbol: &str) -> AppResult<Quote> {
        let inst = to_oanda_instrument(symbol);
        let body = self
            .get_json(
                &format!("/v3/accounts/{}/pricing", self.account_id),
                &[("instruments", &inst)],
            )
            .await?;
        let price = body
            .get("prices")
            .and_then(|p| p.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| AppError::Market("oanda: no pricing".into()))?;
        let bid = price
            .get("bids")
            .and_then(|b| b.as_array())
            .and_then(|a| a.first())
            .and_then(|b| b.get("price"))
            .and_then(dec)
            .ok_or_else(|| AppError::Market("oanda: no bid".into()))?;
        let ask = price
            .get("asks")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first())
            .and_then(|a| a.get("price"))
            .and_then(dec)
            .ok_or_else(|| AppError::Market("oanda: no ask".into()))?;
        let ts = price
            .get("time")
            .and_then(|v| v.as_str())
            .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
            .map(|d| d.with_timezone(&Utc))
            .unwrap_or_else(Utc::now);
        Ok(Quote { symbol: symbol.to_string(), bid, ask, ts })
    }
}

#[async_trait]
impl Broker for OandaClient {
    fn name(&self) -> &'static str { "oanda" }

    /// Place a spot MARKET order with optional `stopLossOnFill` / `takeProfitOnFill`
    /// at the given price levels. `stake` (account-currency notional) is converted
    /// to instrument units by dividing by the current price. Duration is ignored
    /// (spot positions are held until SL/TP or manual close).
    async fn place_order(&self, req: OrderRequest) -> AppResult<BrokerOrder> {
        let inst = to_oanda_instrument(&req.symbol);

        // Need a reference price to convert notional -> units.
        let quote = self.quote(&req.symbol).await?;
        let ref_price = match req.side {
            Side::Buy => quote.ask,
            Side::Sell => quote.bid,
        };
        if ref_price == Decimal::ZERO {
            return Err(AppError::Execution("oanda: zero reference price".into()));
        }
        let units_abs = (req.stake / ref_price).round_dp(0);
        if units_abs == Decimal::ZERO {
            return Err(AppError::Execution("oanda: stake too small for 1 unit".into()));
        }
        let units = match req.side {
            Side::Buy => units_abs,
            Side::Sell => -units_abs,
        };

        let mut order = serde_json::json!({
            "units": units.to_string(),
            "instrument": inst,
            "type": "MARKET",
            "positionFill": "DEFAULT",
        });
        if let Some(obj) = order.as_object_mut() {
            if let Some(sl) = req.stop_loss_price {
                obj.insert("stopLossOnFill".into(), serde_json::json!({ "price": sl.to_string() }));
            }
            if let Some(tp) = req.take_profit_price {
                obj.insert("takeProfitOnFill".into(), serde_json::json!({ "price": tp.to_string() }));
            }
        }

        let body = serde_json::json!({ "order": order });
        let resp = self
            .post_json(&format!("/v3/accounts/{}/orders", self.account_id), &body)
            .await?;
        if let Some(err) = resp.get("errorMessage") {
            return Err(AppError::Execution(format!("oanda order: {err}")));
        }
        let fill = resp
            .get("orderFillTransaction")
            .ok_or_else(|| AppError::Execution("oanda: no orderFillTransaction".into()))?;
        let filled = dec_opt(fill.get("price")).unwrap_or(ref_price);
        let broker_ref = fill
            .get("id")
            .and_then(|i| i.as_str())
            .map(|s| s.to_string())
            .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

        // Fetch the resulting balance.
        let balance_after = self.balance().await.unwrap_or(req.stake);

        Ok(BrokerOrder { broker_ref, filled_price: filled, balance_after })
    }

    async fn balance(&self) -> AppResult<Decimal> {
        let body = self
            .get_json(&format!("/v3/accounts/{}/summary", self.account_id), &[])
            .await?;
        body.get("account")
            .and_then(|a| a.get("balance"))
            .and_then(dec)
            .ok_or_else(|| AppError::Market("oanda: no balance".into()))
    }
}

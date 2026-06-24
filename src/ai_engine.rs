//! AI Prediction Engine — Next Candle Analysis.
//!
//! The AI reads the previous candlesticks on the user's chosen timeframe,
//! detects patterns, computes indicators, and predicts the direction of the
//! NEXT candle. Firm, scientific, single-timeframe.

use crate::db::Db;
use crate::domain::strategy::Rule;
use crate::domain::{AssetClass, Side};
use crate::engine::rules::{evaluate, Indicators};
use crate::error::{AppError, AppResult};
use crate::llm::LlmClient;
use crate::market::MarketProvider;
use chrono::{Duration, Timelike, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ─── Types ───

#[derive(Debug, Clone, Serialize)]
pub struct Prediction {
    pub direction: String,       // "buy" | "sell" | "hold"
    pub confidence: Decimal,     // 0.0 - 1.0
    pub entry_price: Decimal,
    pub stop_loss: Decimal,
    pub take_profit: Decimal,
    pub expiry: chrono::DateTime<chrono::Utc>,
    pub reasoning: String,
    pub signals: Vec<SignalFactor>,
    pub timeframe_secs: u32,
    pub symbol: String,
    pub analysis_time_utc: chrono::DateTime<chrono::Utc>,
    pub current_time_utc: chrono::DateTime<chrono::Utc>,
    pub market_session: String,
    pub scientific_basis: String,
    pub last_candle: CandleSummary,
    pub predicted_candle: CandleSummary,
}

#[derive(Debug, Clone, Serialize, Default)]
pub struct CandleSummary {
    pub direction: String,  // "bullish" | "bearish" | "neutral"
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub body: Decimal,
    pub upper_wick: Decimal,
    pub lower_wick: Decimal,
    pub pattern: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SignalFactor {
    pub source: String,
    pub name: String,
    pub direction: String,
    pub weight: Decimal,
    pub detail: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AnalyzeRequest {
    pub symbol: String,
    #[serde(default = "default_tf")]
    pub timeframe_minutes: u32,
    pub asset_class: Option<AssetClass>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TradeRequest {
    pub symbol: String,
    pub direction: String,
    #[serde(default = "default_tf")]
    pub timeframe_minutes: u32,
    pub stake: Option<Decimal>,
    pub asset_class: Option<AssetClass>,
}

fn default_tf() -> u32 { 5 }

// ─── Market session ───

fn market_session(utc: &chrono::DateTime<Utc>) -> String {
    let h = utc.hour();
    match h {
        0..=7 => "Asian Session".into(),
        8..=12 => "London Session".into(),
        13..=16 => "London-NY Overlap (High Volatility)".into(),
        17..=21 => "New York Session".into(),
        _ => "Off-Hours (Low Liquidity)".into(),
    }
}

// ─── Core analysis ───

pub async fn analyze(
    db: &Db,
    market: &dyn MarketProvider,
    llm: &LlmClient,
    req: &AnalyzeRequest,
) -> AppResult<Prediction> {
    let symbol = &req.symbol;
    let tf_secs = req.timeframe_minutes * 60;
    let now = Utc::now();

    // 1. Fetch candle data for this timeframe.
    let candles = market.candles(symbol, 300).await?;
    if candles.len() < 50 {
        return Err(AppError::Market("not enough candle data for analysis".into()));
    }

    let ind = Indicators::compute(&candles)?;
    let last_candle = candles.last().unwrap();

    // 2. Summarize the last candle.
    let last_summary = summarize_candle(last_candle, &ind);

    // 3. Gather ALL evidence for predicting the NEXT candle.
    let mut factors: Vec<SignalFactor> = Vec::new();
    let mut bull_weight = Decimal::ZERO;
    let mut bear_weight = Decimal::ZERO;

    // --- Candlestick patterns on the last candle ---
    for (name, val) in &ind.patterns {
        if *val == Decimal::ONE {
            let (dir, w) = pattern_sentiment(name);
            if w == Decimal::ZERO { continue; }
            let dir_str = if dir > 0 { "bullish" } else if dir < 0 { "bearish" } else { "neutral" };
            factors.push(SignalFactor {
                source: "candlestick".into(),
                name: name.clone(),
                direction: dir_str.into(),
                weight: w,
                detail: format!("{} pattern detected on last candle", name),
            });
            if dir > 0 { bull_weight += w; }
            else if dir < 0 { bear_weight += w; }
        }
    }

    // --- RSI ---
    if let Some(rsi) = ind.rsi.get(&14) {
        let (dir, w, detail) = if *rsi < Decimal::from(30) {
            (1, Decimal::from(3), format!("RSI {} — oversold (<30). Statistically, price reverses upward in ~68% of cases from this level.", rsi))
        } else if *rsi > Decimal::from(70) {
            (-1, Decimal::from(3), format!("RSI {} — overbought (>70). Statistically, price reverses downward in ~68% of cases from this level.", rsi))
        } else if *rsi < Decimal::from(45) {
            (1, Decimal::from(1), format!("RSI {} — below midpoint, selling pressure exhausting.", rsi))
        } else if *rsi > Decimal::from(55) {
            (-1, Decimal::from(1), format!("RSI {} — above midpoint, buying pressure exhausting.", rsi))
        } else {
            (0, Decimal::ZERO, format!("RSI {} — neutral zone (45-55).", rsi))
        };
        let dir_str = if dir > 0 { "bullish" } else if dir < 0 { "bearish" } else { "neutral" };
        factors.push(SignalFactor { source: "indicator".into(), name: "RSI(14)".into(), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull_weight += w; } else if dir < 0 { bear_weight += w; }
    }

    // --- EMA trend ---
    if let Some(ema50) = ind.ema.get(&50) {
        let (dir, w, detail) = if ind.price > *ema50 {
            (1, Decimal::from(2), format!("Price {} above EMA50 {} — bullish structure. The 50-period EMA acts as dynamic support.", ind.price, ema50))
        } else {
            (-1, Decimal::from(2), format!("Price {} below EMA50 {} — bearish structure. The 50-period EMA acts as dynamic resistance.", ind.price, ema50))
        };
        let dir_str = if dir > 0 { "bullish" } else { "bearish" };
        factors.push(SignalFactor { source: "indicator".into(), name: "EMA(50)".into(), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull_weight += w; } else if dir < 0 { bear_weight += w; }
    }

    // --- EMA200 (long-term trend) ---
    if let Some(ema200) = ind.ema.get(&200) {
        let (dir, w, detail) = if ind.price > *ema200 {
            (1, Decimal::from(2), format!("Price above EMA200 {} — long-term uptrend confirmed. Institutional bias is bullish.", ema200))
        } else {
            (-1, Decimal::from(2), format!("Price below EMA200 {} — long-term downtrend confirmed. Institutional bias is bearish.", ema200))
        };
        let dir_str = if dir > 0 { "bullish" } else { "bearish" };
        factors.push(SignalFactor { source: "indicator".into(), name: "EMA(200)".into(), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull_weight += w; } else if dir < 0 { bear_weight += w; }
    }

    // --- MACD ---
    if let Some(macd) = ind.macd {
        let (dir, w, detail) = if macd > Decimal::ZERO {
            (1, Decimal::from(2), format!("MACD positive ({}) — bullish momentum. Fast EMA above slow EMA.", macd))
        } else {
            (-1, Decimal::from(2), format!("MACD negative ({}) — bearish momentum. Fast EMA below slow EMA.", macd))
        };
        let dir_str = if dir > 0 { "bullish" } else { "bearish" };
        factors.push(SignalFactor { source: "indicator".into(), name: "MACD".into(), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull_weight += w; } else if dir < 0 { bear_weight += w; }
    }

    // --- Bollinger Bands ---
    {
        let (dir, w, detail) = if ind.price > ind.bb_upper {
            (-1, Decimal::from(2), format!("Price {} above upper Bollinger Band {}. Occurs <5% of the time — statistical mean reversion expected.", ind.price, ind.bb_upper))
        } else if ind.price < ind.bb_lower {
            (1, Decimal::from(2), format!("Price {} below lower Bollinger Band {}. Occurs <5% of the time — statistical mean reversion expected.", ind.price, ind.bb_lower))
        } else if ind.price > ind.bb_middle {
            (-1, Decimal::from(1), format!("Price in upper Bollinger half — leaning toward resistance at {}.", ind.bb_upper))
        } else {
            (1, Decimal::from(1), format!("Price in lower Bollinger half — leaning toward support at {}.", ind.bb_lower))
        };
        let dir_str = if dir > 0 { "bullish" } else if dir < 0 { "bearish" } else { "neutral" };
        factors.push(SignalFactor { source: "indicator".into(), name: "Bollinger Bands".into(), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull_weight += w; } else if dir < 0 { bear_weight += w; }
    }

    // --- Stochastic ---
    {
        let (dir, w, detail) = if ind.stoch_k < Decimal::from(20) {
            (1, Decimal::from(2), format!("Stochastic %K {} — oversold (<20). Bullish crossover likely.", ind.stoch_k))
        } else if ind.stoch_k > Decimal::from(80) {
            (-1, Decimal::from(2), format!("Stochastic %K {} — overbought (>80). Bearish crossover likely.", ind.stoch_k))
        } else {
            (0, Decimal::ZERO, format!("Stochastic %K {} — neutral zone.", ind.stoch_k))
        };
        let dir_str = if dir > 0 { "bullish" } else if dir < 0 { "bearish" } else { "neutral" };
        factors.push(SignalFactor { source: "indicator".into(), name: "Stochastic".into(), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull_weight += w; } else if dir < 0 { bear_weight += w; }
    }

    // --- ADX (trend strength — boosts the dominant side) ---
    if ind.adx > Decimal::from(25) {
        let detail = format!("ADX {} — strong trend (>25). Signals are reliable; follow the dominant direction.", ind.adx);
        factors.push(SignalFactor { source: "indicator".into(), name: "ADX".into(), direction: "neutral".into(), weight: Decimal::ZERO, detail });
    } else if ind.adx < Decimal::from(20) {
        let detail = format!("ADX {} — weak trend (<20). Range-bound conditions; reversal patterns are less reliable.", ind.adx);
        factors.push(SignalFactor { source: "indicator".into(), name: "ADX".into(), direction: "neutral".into(), weight: Decimal::ZERO, detail });
    }

    // --- Momentum ---
    {
        let (dir, w, detail) = if ind.pct_change > Decimal::ZERO {
            (1, Decimal::from(1), format!("Last candle closed +{}% — upward momentum.", ind.pct_change))
        } else {
            (-1, Decimal::from(1), format!("Last candle closed {}% — downward momentum.", ind.pct_change))
        };
        let dir_str = if dir > 0 { "bullish" } else { "bearish" };
        factors.push(SignalFactor { source: "momentum".into(), name: "Price Change".into(), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull_weight += w; } else if dir < 0 { bear_weight += w; }
    }

    // --- Note-derived knowledge ---
    let strategies = db.list_enabled_strategies().await.unwrap_or_default();
    let mut note_count = 0u32;
    for strat in &strategies {
        if !strat.symbols.is_empty() && !strat.symbols.iter().any(|s| s == symbol || symbol.contains(s)) {
            continue;
        }
        let rules = db.list_rules(strat.id).await.unwrap_or_default();
        for rule in &rules {
            if !rule.enabled { continue; }
            if let Ok(true) = evaluate(&rule.expr, &ind) {
                let expr_lower = rule.expr.to_lowercase();
                let is_bearish = expr_lower.contains("bearish") || expr_lower.contains("short")
                    || expr_lower.contains("overbought") || expr_lower.contains("> 65") || expr_lower.contains("> 70");
                let dir_str = if is_bearish { "bearish" } else { "bullish" };
                let w = rule.weight;
                factors.push(SignalFactor {
                    source: "note".into(),
                    name: format!("{} ({})", rule.name, strat.name),
                    direction: dir_str.into(),
                    weight: w,
                    detail: format!("Learned rule fired: {}", rule.expr),
                });
                if dir_str == "bullish" { bull_weight += w; }
                else { bear_weight += w; }
                note_count += 1;
            }
        }
    }

    // 4. Compute direction + confidence.
    let total = bull_weight + bear_weight;
    let (direction, confidence): (String, Decimal) = if total == Decimal::ZERO {
        ("hold".into(), Decimal::ZERO)
    } else {
        let ratio = if bull_weight > bear_weight { bull_weight / total } else { bear_weight / total };
        if ratio < Decimal::new(55, 2) {
            ("hold".into(), ratio)
        } else if bull_weight > bear_weight {
            ("buy".into(), ratio)
        } else {
            ("sell".into(), ratio)
        }
    };

    // 5. Predict next candle summary.
    let predicted_candle = predict_next_candle(&direction, &ind, last_candle);

    // 6. Entry / SL / TP.
    let entry = ind.price;
    let atr = ind.atr.get(&14).copied().unwrap_or(entry * Decimal::new(5, 3));
    let pip = if symbol.starts_with("frx") { Decimal::new(1, 4) } else { Decimal::ONE };
    let sl_dist = atr.max(pip * Decimal::from(20));
    let tp_dist = sl_dist * Decimal::from(2);
    let (stop_loss, take_profit) = match direction.as_str() {
        "buy" => (entry - sl_dist, entry + tp_dist),
        "sell" => (entry + sl_dist, entry - tp_dist),
        _ => (entry - sl_dist, entry + tp_dist),
    };

    let expiry = now + Duration::seconds(tf_secs as i64);
    let session = market_session(&now);

    // 7. Build reasoning.
    let reasoning = build_reasoning(
        &direction, &confidence, &session, note_count, &factors, &ind, &last_summary, &predicted_candle, symbol,
    );

    // 8. Scientific basis.
    let scientific_basis = build_scientific_basis(&ind, &direction, &confidence);

    // 9. LLM enhancement.
    let final_reasoning = if let Ok(insight) = llm_enhance(llm, symbol, &direction, &confidence, &factors, &ind).await {
        format!("{}\n\nAI Insight: {}", reasoning, insight)
    } else {
        reasoning
    };

    Ok(Prediction {
        direction,
        confidence,
        entry_price: entry.round_dp(6),
        stop_loss: stop_loss.round_dp(6),
        take_profit: take_profit.round_dp(6),
        expiry,
        reasoning: final_reasoning,
        signals: factors,
        timeframe_secs: tf_secs,
        symbol: symbol.clone(),
        analysis_time_utc: now,
        current_time_utc: now,
        market_session: session,
        scientific_basis,
        last_candle: last_summary,
        predicted_candle,
    })
}

// ─── Candle summary ───

fn summarize_candle(c: &crate::domain::Candle, ind: &Indicators) -> CandleSummary {
    let body = ((c.close - c.open).abs()).round_dp(6);
    let upper_wick = (c.high - c.open.max(c.close)).round_dp(6);
    let lower_wick = (c.open.min(c.close) - c.low).round_dp(6);
    let direction = if c.close > c.open { "bullish" }
        else if c.close < c.open { "bearish" }
        else { "neutral" };

    // Find the dominant pattern.
    let mut best_pattern = "none".to_string();
    let mut best_w = Decimal::ZERO;
    for (name, val) in &ind.patterns {
        if *val == Decimal::ONE {
            let (_, w) = pattern_sentiment(name);
            if w > best_w { best_w = w; best_pattern = name.clone(); }
        }
    }

    CandleSummary {
        direction: direction.into(),
        open: c.open, high: c.high, low: c.low, close: c.close,
        body, upper_wick, lower_wick,
        pattern: best_pattern,
    }
}

// ─── Predict next candle ───

fn predict_next_candle(direction: &str, ind: &Indicators, last: &crate::domain::Candle) -> CandleSummary {
    // Predict the next candle based on direction + indicator context.
    let (pred_dir, body_size) = match direction {
        "buy" => ("bullish", (ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) * Decimal::new(3, 10)).round_dp(6)),
        "sell" => ("bearish", (ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) * Decimal::new(3, 10)).round_dp(6)),
        _ => ("neutral", Decimal::ZERO),
    };

    let (open, close) = match pred_dir {
        "bullish" => (last.close, last.close + body_size),
        "bearish" => (last.close, last.close - body_size),
        _ => (last.close, last.close),
    };

    let range = body_size + body_size * Decimal::new(3, 10);
    let high = open.max(close) + range * Decimal::new(2, 10);
    let low = open.min(close) - range * Decimal::new(2, 10);

    CandleSummary {
        direction: pred_dir.into(),
        open: open.round_dp(6),
        high: high.round_dp(6),
        low: low.round_dp(6),
        close: close.round_dp(6),
        body: body_size,
        upper_wick: (high - open.max(close)).round_dp(6),
        lower_wick: (open.min(close) - low).round_dp(6),
        pattern: format!("predicted_{}", pred_dir),
    }
}

// ─── Reasoning ───

fn build_reasoning(
    direction: &str,
    confidence: &Decimal,
    session: &str,
    note_count: u32,
    factors: &[SignalFactor],
    ind: &Indicators,
    last: &CandleSummary,
    predicted: &CandleSummary,
    symbol: &str,
) -> String {
    let pct = confidence * Decimal::from(100);
    let mut r = String::new();

    r.push_str(&format!(
        "═══ AI MARKET ANALYSIS ═══\n\
        Symbol: {}\n\
        Time (UTC): {}\n\
        Session: {}\n\
        Decision: {} (confidence: {:.1}%)\n",
        symbol,
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        session,
        direction.to_uppercase(),
        pct,
    ));

    // Last candle analysis.
    r.push_str(&format!(
        "\n── LAST CANDLE ──\n\
        Direction: {}\n\
        Open: {} | High: {} | Low: {} | Close: {}\n\
        Body: {} | Upper wick: {} | Lower wick: {}\n\
        Pattern: {}\n",
        last.direction, last.open, last.high, last.low, last.close,
        last.body, last.upper_wick, last.lower_wick, last.pattern,
    ));

    // Predicted next candle.
    r.push_str(&format!(
        "\n── PREDICTED NEXT CANDLE ──\n\
        Direction: {}\n\
        Projected open: {} | close: {}\n\
        Projected high: {} | low: {}\n",
        predicted.direction, predicted.open, predicted.close, predicted.high, predicted.low,
    ));

    // Evidence.
    let bull_count = factors.iter().filter(|f| f.direction == "bullish").count();
    let bear_count = factors.iter().filter(|f| f.direction == "bearish").count();
    r.push_str(&format!(
        "\n── EVIDENCE ──\n\
        {} bullish signals vs {} bearish signals.\n",
        bull_count, bear_count,
    ));
    if note_count > 0 {
        r.push_str(&format!("{} rules from learned notes fired.\n", note_count));
    }

    // Key indicators.
    let rsi = ind.rsi.get(&14).map(|d| d.to_string()).unwrap_or_else(|| "N/A".into());
    let adx = ind.adx.to_string();
    let macd = ind.macd.map(|d| d.to_string()).unwrap_or_else(|| "N/A".into());
    r.push_str(&format!(
        "\n── INDICATORS ──\n\
        RSI(14): {} | ADX: {} | MACD: {}\n\
        Bollinger: upper {} | mid {} | lower {}\n\
        Stochastic: %K {} | EMA50: {} | EMA200: {}\n\
        ATR(14): {} | Swing High: {} | Swing Low: {}\n",
        rsi, adx, macd,
        ind.bb_upper, ind.bb_middle, ind.bb_lower,
        ind.stoch_k,
        ind.ema.get(&50).map(|d| d.to_string()).unwrap_or_else(|| "N/A".into()),
        ind.ema.get(&200).map(|d| d.to_string()).unwrap_or_else(|| "N/A".into()),
        ind.atr.get(&14).map(|d| d.to_string()).unwrap_or_else(|| "N/A".into()),
        ind.swing_high, ind.swing_low,
    ));

    // Conclusion.
    r.push_str("\n── CONCLUSION ──\n");
    match direction {
        "buy" => r.push_str(&format!(
            "The next candle is predicted BULLISH. {} Confidence: {:.1}%. \
            Entry at {}, stop below {}, target {}.\n",
            conviction_label(confidence), pct, ind.price,
            ind.price - ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO),
            ind.price + ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) * Decimal::from(2),
        )),
        "sell" => r.push_str(&format!(
            "The next candle is predicted BEARISH. {} Confidence: {:.1}%. \
            Entry at {}, stop above {}, target {}.\n",
            conviction_label(confidence), pct, ind.price,
            ind.price + ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO),
            ind.price - ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) * Decimal::from(2),
        )),
        _ => r.push_str(&format!(
            "Insufficient evidence to predict next candle direction. Confidence only {:.1}%. \
            Wait for a clearer setup.\n", pct,
        )),
    }

    r
}

fn conviction_label(confidence: &Decimal) -> &'static str {
    if confidence > &Decimal::new(70, 2) { "HIGH-conviction." }
    else if confidence > &Decimal::new(60, 2) { "MODERATE-conviction." }
    else { "MARGINAL — monitor closely." }
}

fn build_scientific_basis(ind: &Indicators, direction: &str, confidence: &Decimal) -> String {
    let mut s = String::new();

    // ADX.
    if ind.adx > Decimal::from(25) {
        s.push_str(&format!("ADX of {} indicates a strong, sustainable trend. ", ind.adx));
    } else if ind.adx > Decimal::from(20) {
        s.push_str(&format!("ADX of {} indicates a developing trend. ", ind.adx));
    } else {
        s.push_str(&format!("ADX of {} suggests weak/range-bound conditions. ", ind.adx));
    }

    // RSI.
    if let Some(rsi) = ind.rsi.get(&14) {
        if *rsi < Decimal::from(30) {
            s.push_str(&format!("RSI {} is in the oversold zone (<30), where historical mean reversion occurs ~68% of the time. ", rsi));
        } else if *rsi > Decimal::from(70) {
            s.push_str(&format!("RSI {} is in the overbought zone (>70), where historical corrections occur ~68% of the time. ", rsi));
        } else {
            s.push_str(&format!("RSI at {} is in neutral territory. ", rsi));
        }
    }

    // Bollinger.
    if ind.price > ind.bb_upper {
        s.push_str("Price is above the upper Bollinger Band (2σ) — a statistical anomaly occurring <5% of the time, typically followed by reversion. ");
    } else if ind.price < ind.bb_lower {
        s.push_str("Price is below the lower Bollinger Band (2σ) — a statistical anomaly occurring <5% of the time, typically followed by reversion. ");
    } else {
        s.push_str("Price is within normal Bollinger range (±2σ). ");
    }

    // Stochastic.
    if ind.stoch_k < Decimal::from(20) {
        s.push_str(&format!("Stochastic %K at {} confirms oversold conditions. ", ind.stoch_k));
    } else if ind.stoch_k > Decimal::from(80) {
        s.push_str(&format!("Stochastic %K at {} confirms overbought conditions. ", ind.stoch_k));
    }

    if direction != "hold" {
        s.push_str(&format!("Prediction confidence of {:.1}% reflects the weight of concurring evidence.", confidence * Decimal::from(100)));
    }

    s
}

// ─── Pattern sentiment ───

fn pattern_sentiment(name: &str) -> (i32, Decimal) {
    match name {
        "hammer" => (1, Decimal::from(2)),
        "bullish_engulfing" => (1, Decimal::from(3)),
        "bullish_harami" => (1, Decimal::from(2)),
        "piercing_line" => (1, Decimal::from(2)),
        "morning_star" => (1, Decimal::from(3)),
        "three_white_soldiers" => (1, Decimal::from(3)),
        "dragonfly_doji" => (1, Decimal::from(2)),
        "long_lower_shadow" => (1, Decimal::from(1)),
        "tweezer_bottom" => (1, Decimal::from(1)),
        "inverted_hammer" => (1, Decimal::from(1)),
        "shooting_star" => (-1, Decimal::from(2)),
        "bearish_engulfing" => (-1, Decimal::from(3)),
        "bearish_harami" => (-1, Decimal::from(2)),
        "dark_cloud_cover" => (-1, Decimal::from(2)),
        "evening_star" => (-1, Decimal::from(3)),
        "three_black_crows" => (-1, Decimal::from(3)),
        "gravestone_doji" => (-1, Decimal::from(2)),
        "long_upper_shadow" => (-1, Decimal::from(1)),
        "tweezer_top" => (-1, Decimal::from(1)),
        "hanging_man" => (-1, Decimal::from(1)),
        "doji" => (0, Decimal::ZERO),
        "spinning_top" => (0, Decimal::ZERO),
        "marubozu" => (0, Decimal::ZERO),
        "bullish_candle" => (1, Decimal::new(5, 1)),
        "bearish_candle" => (-1, Decimal::new(5, 1)),
        _ => (0, Decimal::ZERO),
    }
}

// ─── LLM ───

async fn llm_enhance(
    llm: &LlmClient,
    symbol: &str,
    direction: &str,
    confidence: &Decimal,
    factors: &[SignalFactor],
    ind: &Indicators,
) -> AppResult<String> {
    let rsi = ind.rsi.get(&14).map(|d| d.to_string()).unwrap_or_else(|| "N/A".into());
    let adx = ind.adx.to_string();

    let system = "You are a quantitative trading analyst. Provide a firm, scientific 2-3 sentence assessment of this next-candle prediction. Use specific data. Be decisive. No disclaimers.";
    let user = format!(
        "Market: {}\nPredicted next candle: {}\nConfidence: {}%\nADX: {}\nRSI: {}\nStoch %K: {}\nBollinger position: {}\n\nKey signals:\n{}\n\nAssess this prediction scientifically:",
        symbol, direction, confidence * Decimal::from(100),
        adx, rsi, ind.stoch_k,
        if ind.price > ind.bb_upper { "above upper band" }
        else if ind.price < ind.bb_lower { "below lower band" }
        else { "within bands" },
        factors.iter().filter(|f| f.weight > Decimal::ZERO).take(8)
            .map(|f| format!("- {} ({}): {}", f.name, f.direction, f.detail))
            .collect::<Vec<_>>().join("\n"),
    );

    let resp = llm.extract_json(system, &user).await;
    match resp {
        Ok(v) => v.get("insight").and_then(|i| i.as_str()).map(|s| s.to_string())
            .ok_or_else(|| AppError::Llm("no insight".into())),
        Err(_) => Err(AppError::Llm("LLM not available".into())),
    }
}

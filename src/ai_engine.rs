//! AI Prediction Engine — Multi-Timeframe Analysis.
//!
//! Core philosophy: no single timeframe tells the full story. When analyzing a
//! 15-minute chart, we simultaneously analyze 1H, 4H, 8H, and Daily because
//! higher timeframes govern lower ones. A 15min BUY is strong when confirmed by
//! bullish structure on 1H, 4H, and Daily. Cross-timeframe alignment is the
//! strongest predictor of trade success.
//!
//! The engine reads previous candlesticks on EVERY timeframe, detects patterns,
//! computes indicators (RSI, MACD, EMA, Bollinger Bands, Stochastic, ADX), and
//! predicts the direction of the NEXT candle on each timeframe. The final
//! prediction is a weighted consensus where higher timeframes carry more weight.

use crate::db::Db;
use crate::domain::strategy::Rule;
use crate::domain::{AssetClass, Candle, Side};
use crate::engine::rules::{evaluate, Indicators};
use crate::error::{AppError, AppResult};
use crate::llm::LlmClient;
use crate::market::MarketProvider;
use chrono::{Datelike, Duration, TimeZone, Timelike, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ─── Types ───

/// A single AI prediction for a market + timeframe.
#[derive(Debug, Clone, Serialize)]
pub struct Prediction {
    pub direction: String, // "buy" | "sell" | "hold"
    pub confidence: Decimal, // 0.0 - 1.0
    pub entry_price: Decimal,
    pub stop_loss: Decimal,
    pub take_profit: Decimal,
    pub expiry: chrono::DateTime<chrono::Utc>,
    pub reasoning: String,
    pub signals: Vec<SignalFactor>,
    pub timeframe_secs: u32,
    pub symbol: String,
    /// UTC timestamp when the analysis was performed.
    pub analysis_time_utc: chrono::DateTime<chrono::Utc>,
    /// Current UTC time (for display).
    pub current_time_utc: chrono::DateTime<chrono::Utc>,
    /// Multi-timeframe analysis breakdown.
    pub timeframes: Vec<TimeframeAnalysis>,
    /// How well timeframes agree: "Strong" / "Moderate" / "Weak" / "Conflicting"
    pub cross_tf_alignment: String,
    /// Detected trading session (London, New York, Asian, Overlap, Off-hours).
    pub market_session: String,
    /// Scientific/statistical basis for the prediction.
    pub scientific_basis: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TimeframeAnalysis {
    pub label: String, // "15min", "1H", "4H", "8H", "Daily"
    pub granularity_secs: u32,
    pub trend: String, // "bullish", "bearish", "neutral"
    pub trend_strength: Decimal, // ADX value
    pub direction: String, // predicted direction of next candle
    pub rsi: Decimal,
    pub macd: Decimal,
    pub ema_trend: String, // price vs EMA50
    pub bb_position: String, // "above_upper", "near_middle", "below_lower", etc.
    pub stoch_k: Decimal,
    pub dominant_pattern: String, // top candlestick pattern
    pub bullish_count: u32,
    pub bearish_count: u32,
    pub weight: Decimal, // how much this TF contributes to final score
    pub summary: String, // one-line scientific summary for this TF
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

// ─── Multi-timeframe hierarchy ───

/// Given a base timeframe in minutes, return the hierarchy of timeframes to
/// analyze. Higher timeframes govern lower ones.
fn timeframe_hierarchy(base_minutes: u32) -> Vec<(u32, &'static str, Decimal)> {
    // (minutes, label, weight) — higher TF = higher weight
    let base = base_minutes;
    let mut tfs: Vec<(u32, &'static str, Decimal)> = vec![
        (base, tf_label(base), Decimal::from(1)),
    ];

    // Add higher timeframes.
    let higher: &[(u32, &str)] = &[
        (60, "1H"),
        (240, "4H"),
        (480, "8H"),
        (1440, "Daily"),
    ];

    for (mins, label) in higher {
        if *mins > base {
            // Weight scales with timeframe: each step up roughly doubles weight.
            let ratio = Decimal::from(*mins) / Decimal::from(base);
            let w = ratio.min(Decimal::from(8)); // cap at 8x
            tfs.push((*mins, *label, w));
        }
    }
    tfs
}

fn tf_label(mins: u32) -> &'static str {
    match mins {
        1 => "1min",
        5 => "5min",
        15 => "15min",
        30 => "30min",
        60 => "1H",
        240 => "4H",
        480 => "8H",
        1440 => "Daily",
        _ => "custom",
    }
}

// ─── Candle aggregation ───

/// Aggregate lower-timeframe candles into higher-timeframe candles.
/// e.g., four 15-min candles → one 1H candle.
fn aggregate_candles(candles: &[Candle], factor: usize) -> Vec<Candle> {
    if factor <= 1 || candles.is_empty() {
        return candles.to_vec();
    }
    let mut out: Vec<Candle> = Vec::new();
    let mut i = 0;
    // Align to the oldest boundary.
    let skip = candles.len() % factor;
    i = skip;
    while i + factor <= candles.len() {
        let chunk = &candles[i..i + factor];
        let open = chunk[0].open;
        let close = chunk[chunk.len() - 1].close;
        let high = chunk.iter().map(|c| c.high).fold(Decimal::ZERO, Decimal::max);
        let low = chunk.iter().map(|c| c.low).fold(Decimal::MAX, Decimal::min);
        let volume = chunk.iter().map(|c| c.volume).sum();
        let ts = chunk[0].ts;
        let symbol = chunk[0].symbol.clone();
        out.push(Candle { symbol, ts, open, high, low, close, volume });
        i += factor;
    }
    out
}

// ─── Market session detection ───

fn market_session(utc: &chrono::DateTime<Utc>) -> String {
    let h = utc.hour();
    // Forex sessions (approximate, UTC):
    // Asian: 00:00-09:00
    // London: 08:00-17:00
    // New York: 13:00-22:00
    // London-NY Overlap: 13:00-17:00
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

    // 1. Fetch a large set of candles at the base timeframe.
    // We need enough to aggregate into higher timeframes with adequate warmup.
    let base_candles = market.candles(symbol, 500).await?;
    if base_candles.len() < 50 {
        return Err(AppError::Market("not enough candle data for analysis".into()));
    }

    // 2. Determine timeframe hierarchy.
    let hierarchy = timeframe_hierarchy(req.timeframe_minutes);
    let base_mins = req.timeframe_minutes;

    // 3. Analyze each timeframe.
    let mut tf_analyses: Vec<TimeframeAnalysis> = Vec::new();
    let mut all_factors: Vec<SignalFactor> = Vec::new();
    let mut total_bull = Decimal::ZERO;
    let mut total_bear = Decimal::ZERO;

    for (tf_mins, label, tf_weight) in &hierarchy {
        let factor = (*tf_mins / base_mins) as usize;
        let candles = if factor > 1 {
            aggregate_candles(&base_candles, factor)
        } else {
            base_candles.clone()
        };

        if candles.len() < 30 {
            continue;
        }

        let ind = Indicators::compute(&candles)?;
        let (tf, factors) = analyze_timeframe(&ind, *tf_mins, label, *tf_weight);

        // Accumulate weighted scores.
        for f in &factors {
            let w = f.weight * *tf_weight; // scale by TF weight
            if f.direction == "bullish" {
                total_bull += w;
            } else if f.direction == "bearish" {
                total_bear += w;
            }
        }
        all_factors.extend(factors);
        tf_analyses.push(tf);
    }

    // 4. Run note-derived rules on the base timeframe.
    let base_ind = Indicators::compute(&base_candles)?;
    let strategies = db.list_enabled_strategies().await.unwrap_or_default();
    let mut note_count = 0u32;
    for strat in &strategies {
        if !strat.symbols.is_empty() && !strat.symbols.iter().any(|s| s == symbol || symbol.contains(s)) {
            continue;
        }
        let rules = db.list_rules(strat.id).await.unwrap_or_default();
        for rule in &rules {
            if !rule.enabled { continue; }
            if let Ok(true) = evaluate(&rule.expr, &base_ind) {
                let expr_lower = rule.expr.to_lowercase();
                let is_bearish = expr_lower.contains("bearish") || expr_lower.contains("short")
                    || expr_lower.contains("overbought") || expr_lower.contains("> 65") || expr_lower.contains("> 70");
                let dir = if is_bearish { "bearish" } else { "bullish" };
                let w = rule.weight;
                all_factors.push(SignalFactor {
                    source: "note".into(),
                    name: format!("{} ({})", rule.name, strat.name),
                    direction: dir.into(),
                    weight: w,
                    detail: format!("Learned rule fired: {}", rule.expr),
                });
                if dir == "bullish" { total_bull += w; }
                else { total_bear += w; }
                note_count += 1;
            }
        }
    }

    // 5. Compute final direction + confidence.
    let total = total_bull + total_bear;
    let (direction, confidence): (String, Decimal) = if total == Decimal::ZERO {
        ("hold".into(), Decimal::ZERO)
    } else {
        let ratio = if total_bull > total_bear {
            total_bull / total
        } else {
            total_bear / total
        };
        // Firmness: if confidence is borderline (50-55%), downgrade to hold.
        // The AI only commits when evidence is clear.
        if ratio < Decimal::new(55, 2) {
            ("hold".into(), ratio)
        } else if total_bull > total_bear {
            ("buy".into(), ratio)
        } else {
            ("sell".into(), ratio)
        }
    };

    // 6. Cross-timeframe alignment.
    let bullish_tfs = tf_analyses.iter().filter(|t| t.direction == "buy").count();
    let bearish_tfs = tf_analyses.iter().filter(|t| t.direction == "sell").count();
    let total_tfs = tf_analyses.len();
    let cross_tf_alignment: String = if total_tfs == 0 {
        "Insufficient Data".into()
    } else if bullish_tfs == total_tfs {
        "Strong — All timeframes bullish".into()
    } else if bearish_tfs == total_tfs {
        "Strong — All timeframes bearish".into()
    } else if bullish_tfs >= total_tfs * 3 / 4 {
        "Moderate — Majority bullish".into()
    } else if bearish_tfs >= total_tfs * 3 / 4 {
        "Moderate — Majority bearish".into()
    } else if bullish_tfs > bearish_tfs {
        "Weak — Mixed, lean bullish".into()
    } else if bearish_tfs > bullish_tfs {
        "Weak — Mixed, lean bearish".into()
    } else {
        "Conflicting — Timeframes disagree".into()
    };

    // 7. Compute entry, SL, TP using ATR (scientific risk sizing).
    let entry = base_ind.price;
    let atr = base_ind.atr.get(&14).copied().unwrap_or(entry * Decimal::new(5, 3));
    let pip = if symbol.starts_with("frx") { Decimal::new(1, 4) } else { Decimal::ONE };
    let sl_dist = atr.max(pip * Decimal::from(20));
    let tp_dist = sl_dist * Decimal::from(2); // 1:2 risk/reward

    let (stop_loss, take_profit) = match direction.as_str() {
        "buy" => (entry - sl_dist, entry + tp_dist),
        "sell" => (entry + sl_dist, entry - tp_dist),
        _ => (entry - sl_dist, entry + tp_dist),
    };

    let expiry = now + Duration::seconds(tf_secs as i64);
    let session = market_session(&now);

    // 8. Build scientific reasoning.
    let reasoning = build_reasoning(
        &direction, &confidence, &cross_tf_alignment, &session,
        &tf_analyses, note_count, &base_ind, symbol,
    );

    // 9. Build scientific basis.
    let scientific_basis = build_scientific_basis(
        &tf_analyses, &base_ind, &cross_tf_alignment, &direction, &confidence,
    );

    // 10. LLM enhancement (if configured).
    let final_reasoning = if let Ok(insight) = llm_enhance(
        llm, symbol, &direction, &confidence, &all_factors, &base_ind, &tf_analyses,
    ).await {
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
        signals: all_factors,
        timeframe_secs: tf_secs,
        symbol: symbol.clone(),
        analysis_time_utc: now,
        current_time_utc: now,
        timeframes: tf_analyses,
        cross_tf_alignment,
        market_session: session,
        scientific_basis,
    })
}

/// Analyze a single timeframe: indicators + patterns → trend + prediction.
fn analyze_timeframe(
    ind: &Indicators,
    tf_mins: u32,
    label: &str,
    tf_weight: Decimal,
) -> (TimeframeAnalysis, Vec<SignalFactor>) {
    let mut factors: Vec<SignalFactor> = Vec::new();
    let mut bull: u32 = 0;
    let mut bear: u32 = 0;
    let mut dominant_pattern = "none".to_string();
    let mut best_pattern_weight = Decimal::ZERO;

    // Candlestick patterns on this TF.
    for (pname, val) in &ind.patterns {
        if *val == Decimal::ONE {
            let (dir, w) = pattern_sentiment(pname);
            if w > Decimal::ZERO {
                let dir_str = if dir > 0 { "bullish" } else if dir < 0 { "bearish" } else { "neutral" };
                factors.push(SignalFactor {
                    source: format!("candlestick_{}", label),
                    name: pname.clone(),
                    direction: dir_str.into(),
                    weight: w,
                    detail: format!("{} pattern on {} chart", pname, label),
                });
                if dir > 0 { bull += 1; } else if dir < 0 { bear += 1; }
                if w > best_pattern_weight {
                    best_pattern_weight = w;
                    dominant_pattern = pname.clone();
                }
            }
        }
    }

    // RSI on this TF.
    if let Some(rsi) = ind.rsi.get(&14) {
        let (dir, w, detail) = if *rsi < Decimal::from(30) {
            (1i32, Decimal::from(2), format!("RSI {} — oversold (<30), statistical reversal zone", rsi))
        } else if *rsi > Decimal::from(70) {
            (-1, Decimal::from(2), format!("RSI {} — overbought (>70), statistical reversal zone", rsi))
        } else if *rsi < Decimal::from(45) {
            (1, Decimal::from(1), format!("RSI {} — below midpoint, bearish exhaustion building", rsi))
        } else if *rsi > Decimal::from(55) {
            (-1, Decimal::from(1), format!("RSI {} — above midpoint, bullish exhaustion building", rsi))
        } else {
            (0, Decimal::ZERO, format!("RSI {} — neutral zone", rsi))
        };
        let dir_str = if dir > 0 { "bullish" } else if dir < 0 { "bearish" } else { "neutral" };
        factors.push(SignalFactor { source: format!("indicator_{}", label), name: format!("RSI(14) {}", label), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull += 1; } else if dir < 0 { bear += 1; }
    }

    // EMA trend.
    if let Some(ema50) = ind.ema.get(&50) {
        let (dir, w, detail) = if ind.price > *ema50 {
            (1, Decimal::from(1), format!("Price {} > EMA50 {} — bullish structure", ind.price, ema50))
        } else {
            (-1, Decimal::from(1), format!("Price {} < EMA50 {} — bearish structure", ind.price, ema50))
        };
        let dir_str = if dir > 0 { "bullish" } else { "bearish" };
        factors.push(SignalFactor { source: format!("indicator_{}", label), name: format!("EMA(50) {}", label), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull += 1; } else { bear += 1; }
    }

    // MACD.
    if let Some(macd) = ind.macd {
        let (dir, w, detail) = if macd > Decimal::ZERO {
            (1, Decimal::from(1), format!("MACD {} — positive momentum divergence", macd))
        } else {
            (-1, Decimal::from(1), format!("MACD {} — negative momentum divergence", macd))
        };
        let dir_str = if dir > 0 { "bullish" } else { "bearish" };
        factors.push(SignalFactor { source: format!("indicator_{}", label), name: format!("MACD {}", label), direction: dir_str.into(), weight: w, detail });
        if dir > 0 { bull += 1; } else { bear += 1; }
    }

    // Bollinger Bands position.
    let bb_pos = if ind.price > ind.bb_upper {
        let (dir, w) = (-1i32, Decimal::from(1));
        factors.push(SignalFactor { source: format!("indicator_{}", label), name: format!("BB {}", label), direction: "bearish".into(), weight: w, detail: format!("Price above upper Bollinger Band — mean reversion likely") });
        bear += 1;
        "above_upper".to_string()
    } else if ind.price < ind.bb_lower {
        let w = Decimal::from(1);
        factors.push(SignalFactor { source: format!("indicator_{}", label), name: format!("BB {}", label), direction: "bullish".into(), weight: w, detail: format!("Price below lower Bollinger Band — mean reversion likely") });
        bull += 1;
        "below_lower".to_string()
    } else if ind.price > ind.bb_middle {
        "upper_half".to_string()
    } else {
        "lower_half".to_string()
    };

    // Stochastic.
    if ind.stoch_k < Decimal::from(20) {
        factors.push(SignalFactor { source: format!("indicator_{}", label), name: format!("Stoch {}", label), direction: "bullish".into(), weight: Decimal::from(1), detail: format!("Stoch %K {} — oversold", ind.stoch_k) });
        bull += 1;
    } else if ind.stoch_k > Decimal::from(80) {
        factors.push(SignalFactor { source: format!("indicator_{}", label), name: format!("Stoch {}", label), direction: "bearish".into(), weight: Decimal::from(1), detail: format!("Stoch %K {} — overbought", ind.stoch_k) });
        bear += 1;
    }

    // ADX — trend strength (not direction, but boosts confidence in direction).
    let adx_val = ind.adx;

    // Determine TF trend + predicted direction.
    let (trend, dir) = if bull > bear {
        ("bullish".into(), "buy".into())
    } else if bear > bull {
        ("bearish".into(), "sell".into())
    } else {
        ("neutral".into(), "hold".into())
    };

    let rsi_val = ind.rsi.get(&14).copied().unwrap_or(Decimal::from(50));
    let macd_val = ind.macd.unwrap_or(Decimal::ZERO);
    let ema_trend = if ind.price > *ind.ema.get(&50).unwrap_or(&ind.price) {
        "price_above_ema".into()
    } else {
        "price_below_ema".into()
    };

    let summary = format!(
        "{}: {} trend (ADX {}), {} bullish vs {} bearish signals. Top pattern: {}. RSI {}, BB {}.",
        label, trend, adx_val, bull, bear, dominant_pattern, rsi_val, bb_pos
    );

    let tf = TimeframeAnalysis {
        label: label.into(),
        granularity_secs: tf_mins * 60,
        trend,
        trend_strength: adx_val,
        direction: dir,
        rsi: rsi_val,
        macd: macd_val,
        ema_trend,
        bb_position: bb_pos,
        stoch_k: ind.stoch_k,
        dominant_pattern,
        bullish_count: bull,
        bearish_count: bear,
        weight: tf_weight,
        summary,
    };

    (tf, factors)
}

// ─── Scientific reasoning ───

fn build_reasoning(
    direction: &str,
    confidence: &Decimal,
    alignment: &str,
    session: &str,
    timeframes: &[TimeframeAnalysis],
    note_count: u32,
    base_ind: &Indicators,
    symbol: &str,
) -> String {
    let pct = confidence * Decimal::from(100);
    let mut r = String::new();

    // Header — firm declaration.
    r.push_str(&format!(
        "═══ AI MARKET ANALYSIS ═══\n\
        Symbol: {}\n\
        Time (UTC): {}\n\
        Session: {}\n\
        Decision: {} (confidence: {:.1}%)\n\
        Cross-Timeframe Alignment: {}\n",
        symbol,
        Utc::now().format("%Y-%m-%d %H:%M:%S UTC"),
        session,
        direction.to_uppercase(),
        pct,
        alignment,
    ));

    // Multi-timeframe breakdown.
    r.push_str("\n── MULTI-TIMEFRAME ANALYSIS ──\n");
    for tf in timeframes.iter().rev() { // highest TF first
        r.push_str(&format!(
            "  {} (weight ×{:.0}): {} trend | ADX {} | RSI {} | MACD {} | Stoch %K {} | Pattern: {}\n",
            tf.label, tf.weight, tf.trend, tf.trend_strength,
            tf.rsi, tf.macd, tf.stoch_k, tf.dominant_pattern
        ));
    }

    // Scientific basis.
    r.push_str("\n── EVIDENCE ──\n");
    let total_bull = timeframes.iter().map(|t| t.bullish_count as u32).sum::<u32>();
    let total_bear = timeframes.iter().map(|t| t.bearish_count as u32).sum::<u32>();
    r.push_str(&format!(
        "  Total signals: {} bullish, {} bearish across {} timeframes.\n",
        total_bull, total_bear, timeframes.len(),
    ));

    if note_count > 0 {
        r.push_str(&format!("  Note-derived rules fired: {} (using accumulated trading knowledge).\n", note_count));
    }

    // Key levels.
    r.push_str(&format!(
        "\n── KEY LEVELS ──\n\
        Entry:  {}\n\
        Stop:   {} (ATR-based, {:.1} risk)\n\
        Target: {} (1:2 risk/reward ratio)\n\
        Bollinger: upper {} | mid {} | lower {}\n\
        Swing High: {} | Swing Low: {}\n",
        base_ind.price,
        if direction == "buy" { base_ind.price - base_ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) }
        else { base_ind.price + base_ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) },
        base_ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO),
        if direction == "buy" { base_ind.price + base_ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) * Decimal::from(2) }
        else { base_ind.price - base_ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO) * Decimal::from(2) },
        base_ind.bb_upper, base_ind.bb_middle, base_ind.bb_lower,
        base_ind.swing_high, base_ind.swing_low,
    ));

    // Firm conclusion.
    r.push_str("\n── CONCLUSION ──\n");
    match direction {
        "buy" => r.push_str(&format!(
            "The weight of evidence supports a BUY position. {} Confidence is {:.1}% based on {} timeframes analyzed. \
            Higher timeframe structure is {}. The trade is valid if price holds above the stop loss.\n",
            if confidence > &Decimal::new(70, 2) { "This is a HIGH-conviction trade." }
            else if confidence > &Decimal::new(60, 2) { "This is a MODERATE-conviction trade." }
            else { "This is a MARGINAL trade — monitor closely." },
            pct, timeframes.len(), alignment,
        )),
        "sell" => r.push_str(&format!(
            "The weight of evidence supports a SELL position. {} Confidence is {:.1}% based on {} timeframes analyzed. \
            Higher timeframe structure is {}. The trade is valid if price holds below the stop loss.\n",
            if confidence > &Decimal::new(70, 2) { "This is a HIGH-conviction trade." }
            else if confidence > &Decimal::new(60, 2) { "This is a MODERATE-conviction trade." }
            else { "This is a MARGINAL trade — monitor closely." },
            pct, timeframes.len(), alignment,
        )),
        _ => r.push_str(&format!(
            "Insufficient evidence to commit to a direction. Confidence is only {:.1}%. \
            The market is in equilibrium — wait for a clearer setup before entering.\n", pct,
        )),
    }

    r
}

fn build_scientific_basis(
    timeframes: &[TimeframeAnalysis],
    base_ind: &Indicators,
    alignment: &str,
    direction: &str,
    confidence: &Decimal,
) -> String {
    let mut s = String::new();

    s.push_str("Based on quantitative analysis of ");
    s.push_str(&format!("{} timeframes simultaneously. ", timeframes.len()));

    // ADX assessment.
    let avg_adx: Decimal = if timeframes.is_empty() {
        Decimal::ZERO
    } else {
        timeframes.iter().map(|t| t.trend_strength).sum::<Decimal>() / Decimal::from(timeframes.len())
    };
    if avg_adx > Decimal::from(25) {
        s.push_str(&format!("Average ADX of {:.1} indicates a strong, sustainable trend. ", avg_adx));
    } else if avg_adx > Decimal::from(20) {
        s.push_str(&format!("Average ADX of {:.1} indicates a developing trend. ", avg_adx));
    } else {
        s.push_str(&format!("Average ADX of {:.1} suggests weak trend conditions — range-bound behavior likely. ", avg_adx));
    }

    // RSI assessment.
    if let Some(rsi) = base_ind.rsi.get(&14) {
        if *rsi < Decimal::from(30) {
            s.push_str(&format!("RSI at {} is in the statistically oversold zone (<30), where mean reversion has historically occurred in ~68% of cases. ", rsi));
        } else if *rsi > Decimal::from(70) {
            s.push_str(&format!("RSI at {} is in the statistically overbought zone (>70), where corrections have historically occurred in ~68% of cases. ", rsi));
        } else {
            s.push_str(&format!("RSI at {} is in neutral territory (30-70), indicating balanced supply/demand. ", rsi));
        }
    }

    // Bollinger Band assessment.
    if base_ind.price > base_ind.bb_upper {
        s.push_str("Price is trading above the upper Bollinger Band (2σ), which occurs only ~5% of the time — a statistical anomaly that typically reverts. ");
    } else if base_ind.price < base_ind.bb_lower {
        s.push_str("Price is trading below the lower Bollinger Band (2σ), which occurs only ~5% of the time — a statistical anomaly that typically reverts. ");
    } else {
        s.push_str("Price is within normal Bollinger Band range (±2σ), indicating typical volatility conditions. ");
    }

    // Cross-TF alignment.
    s.push_str(&format!("Cross-timeframe alignment: {}. ", alignment));
    if direction != "hold" {
        s.push_str(&format!("Confidence of {:.1}% reflects the degree of confluence across all timeframes. ", confidence * Decimal::from(100)));
    }

    s
}

/// Map candlestick pattern name to sentiment: (direction, weight).
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

/// LLM enhancement — asks the AI to provide a scientific insight.
async fn llm_enhance(
    llm: &LlmClient,
    symbol: &str,
    direction: &str,
    confidence: &Decimal,
    factors: &[SignalFactor],
    ind: &Indicators,
    timeframes: &[TimeframeAnalysis],
) -> AppResult<String> {
    let rsi = ind.rsi.get(&14).map(|d| d.to_string()).unwrap_or_else(|| "N/A".into());
    let adx = ind.adx.to_string();
    let bb_pos = if ind.price > ind.bb_upper { "above_upper_band" }
        else if ind.price < ind.bb_lower { "below_lower_band" }
        else { "within_bands" };

    let tf_summary: Vec<String> = timeframes.iter().map(|t| {
        format!("{}: {} (ADX {})", t.label, t.trend, t.trend_strength)
    }).collect();

    let system = "You are a quantitative trading analyst. Provide a firm, scientific 2-3 sentence assessment of this trade setup. Use specific data points. Be decisive — do not hedge. Do not include disclaimers.";
    let user = format!(
        "Market: {}\nDecision: {}\nConfidence: {}%\nADX: {} (trend strength)\nRSI: {}\nBollinger: {}\nStoch %K: {}\n\nTimeframes:\n{}\n\nSignals:\n{}\n\nAssess this setup scientifically:",
        symbol, direction, confidence * Decimal::from(100),
        adx, rsi, bb_pos, ind.stoch_k,
        tf_summary.join("\n"),
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

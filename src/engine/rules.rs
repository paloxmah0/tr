use crate::domain::Candle;
use crate::error::{AppError, AppResult};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// Indicator set computed once per evaluation window.
#[derive(Debug, Clone, Default)]
pub struct Indicators {
    pub rsi: HashMap<usize, Decimal>,
    pub ema: HashMap<usize, Decimal>,
    pub sma: HashMap<usize, Decimal>,
    pub atr: HashMap<usize, Decimal>,
    pub macd: Option<Decimal>,
    pub price: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub open: Decimal,
    pub volume: Decimal,
    pub pct_change: Decimal,
    /// Candlestick pattern signals (1.0 = pattern present, 0.0 = not).
    pub patterns: HashMap<String, Decimal>,
    /// Previous candle data for multi-candle patterns.
    pub prev_open: Decimal,
    pub prev_close: Decimal,
    pub prev_high: Decimal,
    pub prev_low: Decimal,
    /// Bollinger Bands (SMA20 ± 2σ).
    pub bb_upper: Decimal,
    pub bb_middle: Decimal,
    pub bb_lower: Decimal,
    /// Stochastic Oscillator (%K, %D).
    pub stoch_k: Decimal,
    pub stoch_d: Decimal,
    /// ADX — trend strength (0-100). >25 = strong trend.
    pub adx: Decimal,
    /// Recent swing high/low (support/resistance).
    pub swing_high: Decimal,
    pub swing_low: Decimal,
    /// Multi-bar sequences.
    pub consecutive_bullish: u32,
    pub consecutive_bearish: u32,
    /// Volatility regime: "expanding", "contracting", "stable".
    pub volatility_regime: String,
    /// Previous ATR (for regime detection).
    pub prev_atr: Decimal,
    /// Bollinger Band width (volatility squeeze detection).
    pub bb_width: Decimal,
    /// Bollinger Band width percentile (0-100, where low = squeeze).
    pub bb_width_pct: Decimal,
    /// Price position: 0.0 = at lower BB, 1.0 = at upper BB.
    pub bb_position_pct: Decimal,
    /// Distance from swing high as % (0 = at swing high, 100 = far from it).
    pub dist_from_swing_high_pct: Decimal,
    /// Distance from swing low as % (0 = at swing low, 100 = far from it).
    pub dist_from_swing_low_pct: Decimal,
    /// Last 5 candle directions for sequence analysis.
    pub candle_sequence: Vec<String>,
    /// Rate of change (5-bar momentum).
    pub roc_5: Decimal,
    /// RSI divergence vs price over recent window: "bullish", "bearish", or "none".
    /// Bullish = price lower low, RSI higher low (reversal up).
    /// Bearish = price higher high, RSI lower high (reversal down).
    pub rsi_divergence: String,
    /// MACD divergence vs price: "bullish", "bearish", or "none".
    pub macd_divergence: String,
    /// Average volume over the lookback vs last bar volume (ratio > 1.5 = spike).
    pub volume_ratio: Decimal,
}

impl Indicators {
    pub fn compute(candles: &[Candle]) -> AppResult<Self> {
        let last = candles.last().ok_or_else(|| AppError::Internal("no candles".into()))?;
        let mut ind = Indicators {
            price: last.close,
            high: last.high,
            low: last.low,
            close: last.close,
            open: last.open,
            volume: last.volume,
            ..Default::default()
        };

        let closes: Vec<Decimal> = candles.iter().map(|c| c.close).collect();
        if closes.len() >= 2 {
            let prev = closes[closes.len() - 2];
            if prev != Decimal::ZERO {
                let pct = (last.close - prev) / prev * Decimal::from(100);
                ind.pct_change = pct.round_dp(4);
            }
        }
        ind.rsi.insert(14, rsi(&closes, 14).unwrap_or(Decimal::from(50)));
        ind.ema.insert(50, ema(&closes, 50).unwrap_or(last.close));
        ind.ema.insert(200, ema(&closes, 200).unwrap_or(last.close));
        ind.sma.insert(50, sma(&closes, 50).unwrap_or(last.close));
        ind.sma.insert(200, sma(&closes, 200).unwrap_or(last.close));
        ind.atr.insert(14, atr(candles, 14).unwrap_or(Decimal::ZERO));
        ind.macd = Some(macd(&closes));

        // Store previous candle for multi-candle patterns.
        if candles.len() >= 2 {
            let prev = &candles[candles.len() - 2];
            ind.prev_open = prev.open;
            ind.prev_close = prev.close;
            ind.prev_high = prev.high;
            ind.prev_low = prev.low;
        }

        // Compute candlestick patterns.
        ind.patterns = detect_patterns(candles);

        // Bollinger Bands (SMA20 ± 2 standard deviations).
        let (bb_upper, bb_middle, bb_lower) = bollinger_bands(&closes, 20);
        ind.bb_upper = bb_upper;
        ind.bb_middle = bb_middle;
        ind.bb_lower = bb_lower;

        // Stochastic Oscillator (%K, %D).
        let (k, d) = stochastic(candles, 14, 3);
        ind.stoch_k = k;
        ind.stoch_d = d;

        // ADX — trend strength.
        ind.adx = adx(candles, 14);

        // Support/Resistance — recent swing high/low (last 20 candles).
        let lookback = candles.len().min(20);
        ind.swing_high = candles[candles.len() - lookback..].iter().map(|c| c.high).fold(Decimal::ZERO, Decimal::max);
        ind.swing_low = candles[candles.len() - lookback..].iter().map(|c| c.low).fold(Decimal::MAX, Decimal::min);

        // Multi-bar sequences: count consecutive bullish/bearish candles.
        ind.candle_sequence = candles.iter().rev().take(5).rev().map(|c| {
            if c.close > c.open { "bullish".into() }
            else if c.close < c.open { "bearish".into() }
            else { "neutral".into() }
        }).collect();
        ind.consecutive_bullish = 0;
        ind.consecutive_bearish = 0;
        for c in candles.iter().rev() {
            if c.close > c.open { ind.consecutive_bullish += 1; } else { break; }
        }
        for c in candles.iter().rev() {
            if c.close < c.open { ind.consecutive_bearish += 1; } else { break; }
        }

        // Volatility regime: compare current ATR to previous ATR.
        let cur_atr = ind.atr.get(&14).copied().unwrap_or(Decimal::ZERO);
        let prev_atr = if candles.len() > 28 {
            atr(&candles[..candles.len() - 14], 14).unwrap_or(cur_atr)
        } else { cur_atr };
        ind.prev_atr = prev_atr;
        ind.volatility_regime = if cur_atr > prev_atr * Decimal::from(12) / Decimal::from(10) {
            "expanding".into()
        } else if cur_atr < prev_atr * Decimal::from(8) / Decimal::from(10) {
            "contracting".into()
        } else {
            "stable".into()
        };

        // Bollinger Band width + percentile.
        let bb_width = (ind.bb_upper - ind.bb_lower).round_dp(6);
        ind.bb_width = bb_width;
        // Compute BB width over last 50 candles to get percentile.
        let mut bb_widths: Vec<Decimal> = Vec::new();
        for i in (20..=closes.len()).rev() {
            let (u, _, l) = bollinger_bands(&closes[i-20..i], 20);
            bb_widths.push((u - l).round_dp(6));
        }
        if !bb_widths.is_empty() && bb_width > Decimal::ZERO {
            let below = bb_widths.iter().filter(|w| **w < bb_width).count();
            ind.bb_width_pct = Decimal::from(below) / Decimal::from(bb_widths.len()) * Decimal::from(100);
        }
        // BB position % (0 = lower band, 1 = upper band).
        let bb_range = ind.bb_upper - ind.bb_lower;
        ind.bb_position_pct = if bb_range != Decimal::ZERO {
            ((ind.price - ind.bb_lower) / bb_range * Decimal::from(100)).round_dp(2)
        } else { Decimal::from(50) };

        // Distance from swing levels.
        let range = ind.swing_high - ind.swing_low;
        if range > Decimal::ZERO {
            ind.dist_from_swing_high_pct = ((ind.swing_high - ind.price) / range * Decimal::from(100)).round_dp(2);
            ind.dist_from_swing_low_pct = ((ind.price - ind.swing_low) / range * Decimal::from(100)).round_dp(2);
        }

        // Rate of change (5-bar).
        if closes.len() >= 6 {
            let past = closes[closes.len() - 6];
            if past != Decimal::ZERO {
                ind.roc_5 = ((last.close - past) / past * Decimal::from(100)).round_dp(4);
            }
        }

        // RSI / MACD divergence vs price (lookback ~30 bars).
        let (rsi_div, macd_div) = detect_divergence(&candles);
        ind.rsi_divergence = rsi_div;
        ind.macd_divergence = macd_div;

        // Volume ratio: last bar volume vs average of prior 20 bars.
        let vol_lookback = candles.len().min(21);
        if vol_lookback > 1 {
            let prior_sum: Decimal = candles[candles.len() - vol_lookback..candles.len() - 1]
                .iter().map(|c| c.volume).sum();
            let avg = prior_sum / Decimal::from(vol_lookback - 1);
            ind.volume_ratio = if avg > Decimal::ZERO {
                (last.volume / avg).round_dp(4)
            } else { Decimal::ONE };
        } else {
            ind.volume_ratio = Decimal::ONE;
        }

        Ok(ind)
    }

    fn lookup(&self, name: &str) -> Option<Decimal> {
        match name {
            "price" | "close" => Some(self.price),
            "high" => Some(self.high),
            "low" => Some(self.low),
            "open" => Some(self.open),
            "volume" => Some(self.volume),
            "pct_change" => Some(self.pct_change),
            "macd" => self.macd,
            "prev_open" => Some(self.prev_open),
            "prev_close" => Some(self.prev_close),
            "prev_high" => Some(self.prev_high),
            "prev_low" => Some(self.prev_low),
            _ => self.patterns.get(name).copied(),
        }
    }
}

fn sma(closes: &[Decimal], period: usize) -> Option<Decimal> {
    if closes.len() < period { return None; }
    let n = Decimal::from(period);
    Some((closes[closes.len() - period..].iter().sum::<Decimal>() / n).round_dp(10))
}

fn ema(closes: &[Decimal], period: usize) -> Option<Decimal> {
    if closes.len() < period { return None; }
    let k = Decimal::from(2) / Decimal::from(period + 1);
    let k = k.round_dp(10);
    let mut ema = sma(closes, period)?;
    for &c in closes.iter().skip(period) {
        ema = (c * k + ema * (Decimal::ONE - k)).round_dp(10);
    }
    Some(ema)
}

fn rsi(closes: &[Decimal], period: usize) -> Option<Decimal> {
    if closes.len() <= period { return None; }
    let mut gains = Decimal::ZERO;
    let mut losses = Decimal::ZERO;
    let window = &closes[closes.len() - period - 1..];
    for w in window.windows(2) {
        let diff = w[1] - w[0];
        if diff > Decimal::ZERO { gains += diff; } else { losses -= diff; }
    }
    let n = Decimal::from(period);
    let avg_gain = (gains / n).round_dp(10);
    let avg_loss = (losses / n).round_dp(10);
    if avg_loss == Decimal::ZERO {
        return Some(Decimal::from(100));
    }
    let rs = (avg_gain / avg_loss).round_dp(10);
    let hundred = Decimal::from(100);
    Some((hundred - hundred / (Decimal::ONE + rs)).round_dp(4))
}

fn atr(candles: &[Candle], period: usize) -> Option<Decimal> {
    if candles.len() <= period { return None; }
    let mut trs = Vec::with_capacity(period);
    let prev_close = candles[candles.len() - period - 1].close;
    for c in &candles[candles.len() - period..] {
        let h_l = c.high - c.low;
        let h_pc = (c.high - prev_close).abs();
        let l_pc = (c.low - prev_close).abs();
        let tr = h_l.max(h_pc).max(l_pc);
        trs.push(tr);
    }
    Some((trs.iter().sum::<Decimal>() / Decimal::from(period)).round_dp(10))
}

fn macd(closes: &[Decimal]) -> Decimal {
    let fast = ema(closes, 12).unwrap_or(Decimal::ZERO);
    let slow = ema(closes, 26).unwrap_or(Decimal::ZERO);
    (fast - slow).round_dp(10)
}

/// Bollinger Bands: SMA(period) ± 2 * standard deviation.
fn bollinger_bands(closes: &[Decimal], period: usize) -> (Decimal, Decimal, Decimal) {
    if closes.len() < period {
        let last = closes.last().copied().unwrap_or(Decimal::ZERO);
        return (last, last, last);
    }
    let slice = &closes[closes.len() - period..];
    let n = Decimal::from(period);
    let mean = (slice.iter().sum::<Decimal>() / n).round_dp(10);
    let var = (slice.iter().map(|p| {
        let d = *p - mean;
        (d * d).round_dp(10)
    }).sum::<Decimal>() / n).round_dp(10);
    let std_f64 = var.to_string().parse::<f64>().unwrap_or(0.0).sqrt();
    let std = Decimal::try_from(std_f64).unwrap_or(Decimal::ZERO).round_dp(10);
    let two = Decimal::from(2);
    (mean + two * std, mean, mean - two * std)
}

/// Stochastic Oscillator: %K = (close - lowest_low) / (highest_high - lowest_low) * 100.
/// %D = SMA(3) of %K.
fn stochastic(candles: &[Candle], period: usize, d_period: usize) -> (Decimal, Decimal) {
    if candles.len() < period + d_period {
        return (Decimal::from(50), Decimal::from(50));
    }
    let mut ks: Vec<Decimal> = Vec::new();
    for i in (period..=candles.len()).rev() {
        let window = &candles[i - period..i];
        let highest = window.iter().map(|c| c.high).fold(Decimal::ZERO, Decimal::max);
        let lowest = window.iter().map(|c| c.low).fold(Decimal::MAX, Decimal::min);
        let close = candles[i - 1].close;
        let range = highest - lowest;
        let k = if range != Decimal::ZERO {
            ((close - lowest) / range * Decimal::from(100)).round_dp(4)
        } else {
            Decimal::from(50)
        };
        ks.push(k);
    }
    let k = ks.first().copied().unwrap_or(Decimal::from(50));
    let d = (ks.iter().take(d_period).sum::<Decimal>() / Decimal::from(d_period.min(ks.len()))).round_dp(4);
    (k, d)
}

/// ADX (Average Directional Index) — measures trend strength, not direction.
/// >25 = strong trend, <20 = weak/no trend.
fn adx(candles: &[Candle], period: usize) -> Decimal {
    if candles.len() < period * 2 + 1 {
        return Decimal::ZERO;
    }
    let mut plus_dms: Vec<Decimal> = Vec::new();
    let mut minus_dms: Vec<Decimal> = Vec::new();
    let mut trs: Vec<Decimal> = Vec::new();

    for i in 1..candles.len() {
        let up_move = candles[i].high - candles[i - 1].high;
        let down_move = candles[i - 1].low - candles[i].low;
        let plus_dm = if up_move > down_move && up_move > Decimal::ZERO { up_move } else { Decimal::ZERO };
        let minus_dm = if down_move > up_move && down_move > Decimal::ZERO { down_move } else { Decimal::ZERO };
        let tr = (candles[i].high - candles[i].low)
            .max((candles[i].high - candles[i - 1].close).abs())
            .max((candles[i].low - candles[i - 1].close).abs());
        plus_dms.push(plus_dm);
        minus_dms.push(minus_dm);
        trs.push(tr);
    }

    if trs.len() < period {
        return Decimal::ZERO;
    }

    // Wilder's smoothing.
    let n = Decimal::from(period);
    let mut atr_sum = trs[..period].iter().sum::<Decimal>();
    let mut plus_dm_sum = plus_dms[..period].iter().sum::<Decimal>();
    let mut minus_dm_sum = minus_dms[..period].iter().sum::<Decimal>();

    let mut dxs: Vec<Decimal> = Vec::new();
    for i in period..trs.len() {
        let atr = (atr_sum / n).round_dp(10);
        if atr != Decimal::ZERO {
            let plus_di = (plus_dm_sum / n * Decimal::from(100) / atr).round_dp(4);
            let minus_di = (minus_dm_sum / n * Decimal::from(100) / atr).round_dp(4);
            let di_sum = plus_di + minus_di;
            let dx = if di_sum != Decimal::ZERO {
                ((plus_di - minus_di).abs() / di_sum * Decimal::from(100)).round_dp(4)
            } else { Decimal::ZERO };
            dxs.push(dx);
        }
        atr_sum = atr_sum - atr_sum / n + trs[i];
        plus_dm_sum = plus_dm_sum - plus_dm_sum / n + plus_dms[i];
        minus_dm_sum = minus_dm_sum - minus_dm_sum / n + minus_dms[i];
    }

    if dxs.is_empty() {
        return Decimal::ZERO;
    }
    let take = dxs.len().min(period);
    (dxs[dxs.len() - take..].iter().sum::<Decimal>() / Decimal::from(take)).round_dp(2)
}

// ---- Enhanced Candlestick Pattern Detection ----
// Strong reading: each pattern is context-aware (trend, volatility, position)
// and reports a STRENGTH value (0.0 to 1.0), not just binary present/absent.
//
// Enhancements over basic detection:
// 1. Candle body size relative to ATR (big body = conviction, tiny body = noise)
// 2. Wick rejection strength (how hard did buyers/sellers reject a level?)
// 3. Trend context (hammer in downtrend = strong, hammer in uptrend = weak)
// 4. Multi-candle momentum sequences (3+ bars same direction = exhaustion)
// 5. Failed breakout detection (price broke a level then closed back = trap)
// 6. Inside bar / narrowing range (compression before breakout)

fn detect_patterns(candles: &[Candle]) -> HashMap<String, Decimal> {
    let mut p = HashMap::new();
    if candles.is_empty() { return p; }
    let c = &candles[candles.len() - 1];
    let o = c.open; let h = c.high; let l = c.low; let cl = c.close;
    let body = ((cl - o).abs()).round_dp(10);
    let range = ((h - l).abs()).round_dp(10);
    let upper_shadow = ((h - o.max(cl)).abs()).round_dp(10);
    let lower_shadow = ((o.min(cl) - l).abs()).round_dp(10);
    let two = Decimal::from(2);

    // Compute ATR for body-strength comparison (big body vs average = conviction).
    let atr_val = atr(candles, 14).unwrap_or(range);
    let body_vs_atr = if atr_val > Decimal::ZERO { body / atr_val } else { Decimal::ONE };

    // Determine recent trend (last 10 candles) for context.
    let trend = if candles.len() >= 10 {
        let recent = &candles[candles.len() - 10..];
        let starts = recent[0].close;
        let ends = recent.last().unwrap().close;
        if ends > starts { "up" } else if ends < starts { "down" } else { "flat" }
    } else { "flat" };

    if range == Decimal::ZERO {
        p.insert("doji".into(), Decimal::ONE);
        return p;
    }

    let body_pct = (body / range).round_dp(6);
    let upper_pct = (upper_shadow / range).round_dp(6);
    let lower_pct = (lower_shadow / range).round_dp(6);
    let is_bull = cl > o;
    let is_bear = cl < o;

    // ─── Single-candle patterns with STRENGTH scoring ───

    // Hammer / Hanging Man: long lower wick (buyers rejected lows).
    // STRONG if: in a downtrend (reversal), body is small, lower wick >= 2x body.
    // Strength boosted by wick-to-body ratio and downtrend context.
    if lower_shadow >= body * two && upper_shadow <= body * Decimal::new(1, 1) && body > Decimal::ZERO {
        let wick_ratio = if body > Decimal::ZERO { lower_shadow / body } else { Decimal::ZERO };
        let trend_boost = if trend == "down" { Decimal::new(15, 1) } else { Decimal::ZERO };
        let strength = (Decimal::new(5, 1) + wick_ratio * Decimal::new(2, 1) + trend_boost).min(Decimal::ONE);
        p.insert("hammer".into(), Decimal::ONE); // present flag
        p.insert("hammer_strength".into(), strength);
    }
    p.insert("hammer".into(), p.get("hammer").copied().unwrap_or(Decimal::ZERO));

    // Inverted Hammer / Shooting Star: long upper wick.
    // BULLISH if in a downtrend (inverted hammer = buyers tried to push up).
    // BEARISH if in an uptrend (shooting star = sellers rejected highs).
    if upper_shadow >= body * two && lower_shadow <= body * Decimal::new(1, 1) && body > Decimal::ZERO {
        if trend == "down" {
            p.insert("inverted_hammer".into(), Decimal::ONE);
        } else if trend == "up" {
            let wick_ratio = if body > Decimal::ZERO { upper_shadow / body } else { Decimal::ZERO };
            p.insert("shooting_star".into(), Decimal::ONE);
            p.insert("shooting_star_strength".into(), (Decimal::new(5, 1) + wick_ratio * Decimal::new(2, 1)).min(Decimal::ONE));
        }
    }

    // Doji: body <= 5% of range. Strength = how small the body is.
    let is_doji = body_pct <= Decimal::new(5, 2);
    p.insert("doji".into(), bool_dec(is_doji));

    // Dragonfly Doji: doji with long lower wick (strong bullish rejection at bottom).
    if is_doji && lower_pct >= Decimal::new(60, 2) && trend == "down" {
        p.insert("dragonfly_doji".into(), Decimal::ONE);
        p.insert("dragonfly_strength".into(), lower_pct / Decimal::from(100));
    }

    // Gravestone Doji: doji with long upper wick (strong bearish rejection at top).
    if is_doji && upper_pct >= Decimal::new(60, 2) && trend == "up" {
        p.insert("gravestone_doji".into(), Decimal::ONE);
        p.insert("gravestone_strength".into(), upper_pct / Decimal::from(100));
    }

    // Bullish/Bearish candle — only flagged as significant if body > 0.5 ATR (conviction).
    p.insert("bullish_candle".into(), bool_dec(is_bull && body_vs_atr > Decimal::new(5, 1)));
    p.insert("bearish_candle".into(), bool_dec(is_bear && body_vs_atr > Decimal::new(5, 1)));
    p.insert("weak_bullish".into(), bool_dec(is_bull && body_vs_atr <= Decimal::new(5, 1)));
    p.insert("weak_bearish".into(), bool_dec(is_bear && body_vs_atr <= Decimal::new(5, 1)));

    // Marubozu (full-body candle = maximum conviction). Strength = body% of range.
    let is_marubozu = body_pct >= Decimal::new(90, 2);
    p.insert("marubozu".into(), bool_dec(is_marubozu));
    if is_marubozu {
        p.insert("marubozu_strength".into(), body_pct);
        p.insert(if is_bull { "marubozu_bull".into() } else { "marubozu_bear".into() }, Decimal::ONE);
    }

    // Spinning Top: small body + long wicks both sides = indecision.
    let is_spinning_top = body_pct <= Decimal::new(30, 2) && upper_shadow > body && lower_shadow > body;
    p.insert("spinning_top".into(), bool_dec(is_spinning_top));

    // Hanging Man: hammer shape but in an UPTREND (bearish, not bullish).
    if lower_shadow >= body * two && trend == "up" && body > Decimal::ZERO {
        p.insert("hanging_man".into(), Decimal::ONE);
    }

    // Long shadow rejections (strength = wick % of range).
    p.insert("long_upper_shadow".into(), bool_dec(upper_pct >= Decimal::new(66, 2)));
    p.insert("long_lower_shadow".into(), bool_dec(lower_pct >= Decimal::new(66, 2)));

    // ─── Two-candle patterns (engulfing, harami, piercing, dark cloud) ───
    if candles.len() >= 2 {
        let prev = &candles[candles.len() - 2];
        let po = prev.open; let pc = prev.close; let ph = prev.high; let pl = prev.low;
        let prev_bullish = pc > po;
        let prev_bearish = pc < po;
        let prev_body = ((pc - po).abs()).round_dp(10);
        let prev_range = ((ph - pl).abs()).round_dp(10);

        // Bullish Engulfing: STRONG if prev body is large (real selling) and
        // current body engulfs it completely (buyers overwhelmed sellers).
        let is_bull_engulf = prev_bearish && cl > o && o <= pc && cl >= po;
        p.insert("bullish_engulfing".into(), bool_dec(is_bull_engulf));
        if is_bull_engulf {
            // Strength: how much bigger is the engulfing body vs the prev body?
            let dominance = if prev_body > Decimal::ZERO { body / prev_body } else { Decimal::ONE };
            p.insert("bullish_engulfing_strength".into(), dominance.min(Decimal::from(2)) / Decimal::from(2));
        }

        // Bearish Engulfing: STRONG if prev body is large (real buying) and
        // current body engulfs it (sellers overwhelmed buyers).
        let is_bear_engulf = prev_bullish && cl < o && o >= pc && cl <= po;
        p.insert("bearish_engulfing".into(), bool_dec(is_bear_engulf));
        if is_bear_engulf {
            let dominance = if prev_body > Decimal::ZERO { body / prev_body } else { Decimal::ONE };
            p.insert("bearish_engulfing_strength".into(), dominance.min(Decimal::from(2)) / Decimal::from(2));
        }

        // Bullish Harami: prev big bearish, curr small bullish inside prev.
        let is_bull_harami = prev_bearish && prev_body > body * two && cl > o && o >= pc && cl <= po;
        p.insert("bullish_harami".into(), bool_dec(is_bull_harami));

        // Bearish Harami: prev big bullish, curr small bearish inside prev.
        let is_bear_harami = prev_bullish && prev_body > body * two && cl < o && o <= pc && cl >= po;
        p.insert("bearish_harami".into(), bool_dec(is_bear_harami));

        // Piercing Line: prev bearish, curr opens below prev low, closes above prev midpoint.
        let prev_mid = ((po + pc) / two).round_dp(10);
        let is_piercing = prev_bearish && o < pl && cl > prev_mid && cl < po;
        p.insert("piercing_line".into(), bool_dec(is_piercing));
        if is_piercing {
            // Strength: how far past the midpoint did it close?
            let penetration = if po != prev_mid { (cl - prev_mid) / (po - prev_mid).abs() } else { Decimal::ZERO };
            p.insert("piercing_strength".into(), penetration.min(Decimal::ONE));
        }

        // Dark Cloud Cover: prev bullish, curr opens above prev high, closes below midpoint.
        let is_dark_cloud = prev_bullish && o > ph && cl < prev_mid && cl > po;
        p.insert("dark_cloud_cover".into(), bool_dec(is_dark_cloud));

        // Tweezer Bottom/Top: matching lows/highs (within 0.5% of range = very tight).
        let tweezer_tol = range * Decimal::new(5, 3); // 0.5%
        p.insert("tweezer_bottom".into(), bool_dec(((l - pl).abs() <= tweezer_tol)));
        p.insert("tweezer_top".into(), bool_dec(((h - ph).abs() <= tweezer_tol)));

        // ─── Inside Bar: current candle is completely inside the previous candle's range. ───
        // This is a compression pattern — breakout direction tells you the next move.
        let is_inside = h < ph && l > pl;
        p.insert("inside_bar".into(), bool_dec(is_inside));

        // ─── Failed Breakout (Bull Trap): price broke above prev high then closed back below. ───
        // STRONG bearish signal — buyers tried to break out, sellers rejected them.
        let bull_trap = h > ph && cl < ph && is_bear;
        p.insert("failed_breakout_up".into(), bool_dec(bull_trap));
        if bull_trap {
            let rejection = (h - cl) / range; // how far it fell back from the high
            p.insert("failed_breakout_up_strength".into(), rejection);
        }

        // ─── Failed Breakout (Bear Trap): price broke below prev low then closed back above. ───
        // STRONG bullish signal — sellers tried to break down, buyers rejected them.
        let bear_trap = l < pl && cl > pl && is_bull;
        p.insert("failed_breakout_down".into(), bool_dec(bear_trap));
        if bear_trap {
            let rejection = (cl - l) / range;
            p.insert("failed_breakout_down_strength".into(), rejection);
        }
    }

    // ─── Three-candle patterns ───
    if candles.len() >= 3 {
        let prev2 = &candles[candles.len() - 3];
        let prev = &candles[candles.len() - 2];
        let po2 = prev2.open; let pc2 = prev2.close;
        let po = prev.open; let pc = prev.close;

        // Morning Star: bearish → small body → bullish closing into first body.
        let prev_mid2 = ((po2 + pc2) / two).round_dp(10);
        let is_morning_star = pc2 < po2 && (pc - po).abs() < (pc2 - po2).abs() * Decimal::new(5, 10) && cl > o && cl > prev_mid2;
        p.insert("morning_star".into(), bool_dec(is_morning_star));
        if is_morning_star {
            // Strength: how far the third candle closes into the first candle's body.
            let penetration = if po2 != prev_mid2 { (cl - prev_mid2) / (po2 - prev_mid2).abs() } else { Decimal::ZERO };
            p.insert("morning_star_strength".into(), penetration.min(Decimal::ONE));
        }

        // Evening Star: bullish → small body → bearish closing into first body.
        let is_evening_star = pc2 > po2 && (pc - po).abs() < (pc2 - po2).abs() * Decimal::new(5, 10) && cl < o && cl < prev_mid2;
        p.insert("evening_star".into(), bool_dec(is_evening_star));
        if is_evening_star {
            let penetration = if pc2 != prev_mid2 { (prev_mid2 - cl) / (pc2 - prev_mid2).abs() } else { Decimal::ZERO };
            p.insert("evening_star_strength".into(), penetration.min(Decimal::ONE));
        }

        // Three White Soldiers: three bullish candles with rising closes. STRONG in a downtrend.
        let is_three_soldiers = pc2 > po2 && pc > po && cl > o && pc > pc2 && cl > pc;
        p.insert("three_white_soldiers".into(), bool_dec(is_three_soldiers));
        if is_three_soldiers && trend == "down" {
            p.insert("three_soldiers_reversal".into(), Decimal::ONE); // extra flag: reversal context
        }

        // Three Black Crows: three bearish candles with falling closes. STRONG in an uptrend.
        let is_three_crows = pc2 < po2 && pc < po && cl < o && pc < pc2 && cl < pc;
        p.insert("three_black_crows".into(), bool_dec(is_three_crows));
        if is_three_crows && trend == "up" {
            p.insert("three_crows_reversal".into(), Decimal::ONE);
        }

        // ─── Three-bar reversal: strong 3-bar pattern where the third candle
        // engulfs or strongly reverses the prior two. High conviction.
        let three_bar_bull = pc2 < po2 && pc < po && cl > o && cl > po2; // 2 bearish then bullish breaking above first
        p.insert("three_bar_bull_reversal".into(), bool_dec(three_bar_bull));
        let three_bar_bear = pc2 > po2 && pc > po && cl < o && cl < po2;
        p.insert("three_bar_bear_reversal".into(), bool_dec(three_bar_bear));
    }

    // ─── Four-candle momentum / exhaustion sequences ───
    if candles.len() >= 4 {
        let last4 = &candles[candles.len() - 4..];
        let all_bull = last4.iter().all(|c| c.close > c.open);
        let all_bear = last4.iter().all(|c| c.close < c.open);
        // 4+ same-direction candles = exhaustion (reversal probability rises).
        if all_bull { p.insert("four_bull_exhaustion".into(), Decimal::ONE); }
        if all_bear { p.insert("four_bear_exhaustion".into(), Decimal::ONE); }
    }

    // ─── Five-candle patterns ───
    if candles.len() >= 5 {
        let last5 = &candles[candles.len() - 5..];
        let all_bull5 = last5.iter().all(|c| c.close > c.open);
        let all_bear5 = last5.iter().all(|c| c.close < c.open);
        if all_bull5 { p.insert("five_bull_exhaustion".into(), Decimal::ONE); }
        if all_bear5 { p.insert("five_bear_exhaustion".into(), Decimal::ONE); }

        // ─── Narrowing range (compression): each candle's range is smaller than the last.
        // Strong breakout predictor — energy is building.
        let ranges: Vec<Decimal> = last5.iter().map(|c| (c.high - c.low).abs()).collect();
        let narrowing = ranges.windows(2).all(|w| w[1] < w[0]);
        if narrowing { p.insert("narrowing_range".into(), Decimal::ONE); }

        // ─── Expanding range (volatility explosion): each candle's range is bigger.
        let expanding = ranges.windows(2).all(|w| w[1] > w[0]);
        if expanding { p.insert("expanding_range".into(), Decimal::ONE); }
    }

    // ─── CHART PATTERNS (multi-bar structural patterns) ───
    // These are the highest-probability patterns in technical analysis.
    // They use 10-20 bars of structure, not just 2-3 candles.
    if candles.len() >= 20 {
        let lookback = 20.min(candles.len());
        let window = &candles[candles.len() - lookback..];

        // Find the two highest highs and two lowest lows in the window.
        let mut highs: Vec<(usize, Decimal)> = window.iter().enumerate().map(|(i, c)| (i, c.high)).collect();
        let mut lows: Vec<(usize, Decimal)> = window.iter().enumerate().map(|(i, c)| (i, c.low)).collect();
        highs.sort_by(|a, b| b.1.cmp(&a.1));
        lows.sort_by(|a, b| a.1.cmp(&b.1));

        let atr_val = atr(candles, 14).unwrap_or(Decimal::ZERO);
        let tolerance = atr_val * Decimal::new(5, 1); // 0.5x ATR

        // ─── Double Top: two highs at similar levels with a dip between ───
        // Bearish reversal — price failed to break higher twice.
        if highs.len() >= 2 {
            let (h1_idx, h1) = highs[0];
            let (h2_idx, h2) = highs[1];
            let idx_diff = (h1_idx as i64 - h2_idx as i64).unsigned_abs();
            if idx_diff >= 3 && (h1 - h2).abs() <= tolerance {
                // Confirm there's a dip between them (the "valley")
                let valley_low = window[h1_idx.min(h2_idx)..h1_idx.max(h2_idx)]
                    .iter().map(|c| c.low).fold(Decimal::MAX, Decimal::min);
                if h1 - valley_low > tolerance {
                    p.insert("double_top".into(), Decimal::ONE);
                }
            }
        }

        // ─── Double Bottom: two lows at similar levels with a bump between ───
        // Bullish reversal — price failed to break lower twice.
        if lows.len() >= 2 {
            let (l1_idx, l1) = lows[0];
            let (l2_idx, l2) = lows[1];
            let idx_diff = (l1_idx as i64 - l2_idx as i64).unsigned_abs();
            if idx_diff >= 3 && (l1 - l2).abs() <= tolerance {
                let valley_high = window[l1_idx.min(l2_idx)..l1_idx.max(l2_idx)]
                    .iter().map(|c| c.high).fold(Decimal::ZERO, Decimal::max);
                if valley_high - l1 > tolerance {
                    p.insert("double_bottom".into(), Decimal::ONE);
                }
            }
        }

        // ─── Higher Highs / Lower Lows sequence (trend structure) ───
        let quarter = lookback / 4;
        if quarter >= 3 {
            let q1_high = window[0..quarter].iter().map(|c| c.high).fold(Decimal::ZERO, Decimal::max);
            let q4_high = window[lookback - quarter..].iter().map(|c| c.high).fold(Decimal::ZERO, Decimal::max);
            let q1_low = window[0..quarter].iter().map(|c| c.low).fold(Decimal::MAX, Decimal::min);
            let q4_low = window[lookback - quarter..].iter().map(|c| c.low).fold(Decimal::MAX, Decimal::min);

            if q4_high > q1_high && q4_low > q1_low {
                p.insert("uptrend_structure".into(), Decimal::ONE);
            } else if q4_high < q1_high && q4_low < q1_low {
                p.insert("downtrend_structure".into(), Decimal::ONE);
            }
        }

        // ─── Support/Resistance level breach ───
        // Check if the last candle closed above/below a significant prior level.
        let prior_window = &window[..window.len() - 1];
        let resistance = prior_window.iter().map(|c| c.high).fold(Decimal::ZERO, Decimal::max);
        let support = prior_window.iter().map(|c| c.low).fold(Decimal::MAX, Decimal::min);
        if cl > resistance {
            p.insert("resistance_breakout".into(), Decimal::ONE);
        }
        if cl < support {
            p.insert("support_breakdown".into(), Decimal::ONE);
        }
    }

    p
}

// ---- Divergence detection ----
// Compares two recent swing extremes in price against the RSI/MACD readings
// at those points. Divergence = price makes a new extreme while the oscillator
// fails to — a high-probability reversal signal.

/// RSI computed over the `period` bars ending just before `end` (exclusive).
fn rsi_at(closes: &[Decimal], end: usize, period: usize) -> Option<Decimal> {
    if end < period + 1 || end > closes.len() { return None; }
    rsi(&closes[..end], period)
}

/// MACD (fast-slow) computed over bars ending just before `end`.
fn macd_at(closes: &[Decimal], end: usize) -> Option<Decimal> {
    if end < 26 || end > closes.len() { return None; }
    let fast = ema(&closes[..end], 12)?;
    let slow = ema(&closes[..end], 26)?;
    Some((fast - slow).round_dp(10))
}

fn detect_divergence(candles: &[Candle]) -> (String, String) {
    let n = candles.len();
    if n < 35 { return ("none".into(), "none".into()); }
    let lookback = n.min(35);
    let start = n - lookback;
    let mid = start + lookback / 2;
    let closes: Vec<Decimal> = candles.iter().map(|c| c.close).collect();

    // Bearish: second-half high > first-half high, but RSI/MACD lower.
    let (h1_idx, h1) = (start..mid)
        .map(|i| (i, candles[i].high)).max_by(|a, b| a.1.cmp(&b.1)).unwrap_or((0, Decimal::ZERO));
    let (h2_idx, h2) = (mid..n)
        .map(|i| (i, candles[i].high)).max_by(|a, b| a.1.cmp(&b.1)).unwrap_or((0, Decimal::ZERO));
    let bear_rsi = h2 > h1 && h1_idx > 0;
    let bear_rsi = if bear_rsi {
        let r1 = rsi_at(&closes, h1_idx + 1, 14);
        let r2 = rsi_at(&closes, h2_idx + 1, 14);
        match (r1, r2) { (Some(a), Some(b)) => b < a, _ => false }
    } else { false };
    let bear_macd = if h2 > h1 && h1_idx > 0 {
        let m1 = macd_at(&closes, h1_idx + 1);
        let m2 = macd_at(&closes, h2_idx + 1);
        match (m1, m2) { (Some(a), Some(b)) => b < a, _ => false }
    } else { false };

    // Bullish: second-half low < first-half low, but RSI/MACD higher.
    let (l1_idx, l1) = (start..mid)
        .map(|i| (i, candles[i].low)).min_by(|a, b| a.1.cmp(&b.1)).unwrap_or((0, Decimal::MAX));
    let (l2_idx, l2) = (mid..n)
        .map(|i| (i, candles[i].low)).min_by(|a, b| a.1.cmp(&b.1)).unwrap_or((0, Decimal::MAX));
    let bull_rsi = l2 < l1;
    let bull_rsi = if bull_rsi {
        let r1 = rsi_at(&closes, l1_idx + 1, 14);
        let r2 = rsi_at(&closes, l2_idx + 1, 14);
        match (r1, r2) { (Some(a), Some(b)) => b > a, _ => false }
    } else { false };
    let bull_macd = if l2 < l1 {
        let m1 = macd_at(&closes, l1_idx + 1);
        let m2 = macd_at(&closes, l2_idx + 1);
        match (m1, m2) { (Some(a), Some(b)) => b > a, _ => false }
    } else { false };

    let rsi_div = if bear_rsi { "bearish" } else if bull_rsi { "bullish" } else { "none" };
    let macd_div = if bear_macd { "bearish" } else if bull_macd { "bullish" } else { "none" };
    (rsi_div.into(), macd_div.into())
}

fn bool_dec(b: bool) -> Decimal {
    if b { Decimal::ONE } else { Decimal::ZERO }
}

// ---- Mini expression evaluator ----
// Supports: numbers, function(...), +/-/*//, < <= > >= == !=, and/or/not, parens.

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(Decimal),
    Ident(String),
    LParen, RParen, Comma,
    Lt, Le, Gt, Ge, Eq, Ne,
    And, Or, Not,
    Plus, Minus, Star, Slash,
}

fn tokenize(s: &str) -> AppResult<Vec<Tok>> {
    let mut toks = Vec::new();
    let bytes: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_whitespace() { i += 1; continue; }
        match c {
            '(' => { toks.push(Tok::LParen); i += 1; }
            ')' => { toks.push(Tok::RParen); i += 1; }
            ',' => { toks.push(Tok::Comma); i += 1; }
            '<' => {
                if i + 1 < bytes.len() && bytes[i + 1] == '=' { toks.push(Tok::Le); i += 2; }
                else { toks.push(Tok::Lt); i += 1; }
            }
            '>' => {
                if i + 1 < bytes.len() && bytes[i + 1] == '=' { toks.push(Tok::Ge); i += 2; }
                else { toks.push(Tok::Gt); i += 1; }
            }
            '=' => {
                if i + 1 < bytes.len() && bytes[i + 1] == '=' { toks.push(Tok::Eq); i += 2; }
                else { return Err(AppError::BadRequest("expected '=='".into())); }
            }
            '!' => {
                if i + 1 < bytes.len() && bytes[i + 1] == '=' { toks.push(Tok::Ne); i += 2; }
                else { return Err(AppError::BadRequest("expected '!='".into())); }
            }
            '+' => { toks.push(Tok::Plus); i += 1; }
            '-' => { toks.push(Tok::Minus); i += 1; }
            '*' => { toks.push(Tok::Star); i += 1; }
            '/' => { toks.push(Tok::Slash); i += 1; }
            _ if c.is_alphabetic() || c == '_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_alphanumeric() || bytes[i] == '_' || bytes[i] == '.') {
                    i += 1;
                }
                let word: String = bytes[start..i].iter().collect();
                match word.as_str() {
                    "and" | "AND" => toks.push(Tok::And),
                    "or" | "OR" => toks.push(Tok::Or),
                    "not" | "NOT" => toks.push(Tok::Not),
                    _ => toks.push(Tok::Ident(word)),
                }
            }
            _ if c.is_ascii_digit() || c == '.' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_digit() || bytes[i] == '.') {
                    i += 1;
                }
                let num: String = bytes[start..i].iter().collect();
                let d = Decimal::from_str_exact(&num)
                    .map_err(|e| AppError::BadRequest(format!("bad number {num}: {e}")))?;
                toks.push(Tok::Num(d));
            }
            _ => return Err(AppError::BadRequest(format!("unexpected char '{c}'"))),
        }
    }
    Ok(toks)
}

#[derive(Debug, Clone)]
enum Val { Num(Decimal), Bool(bool) }

fn bool_op(v: Val) -> bool {
    match v { Val::Bool(b) => b, Val::Num(n) => n != Decimal::ZERO }
}
fn num_op(v: Val) -> AppResult<Decimal> {
    match v {
        Val::Num(n) => Ok(n),
        Val::Bool(b) => Ok(if b { Decimal::ONE } else { Decimal::ZERO }),
    }
}

pub fn evaluate(expr: &str, ind: &Indicators) -> AppResult<bool> {
    let toks = tokenize(expr)?;
    let mut p = EvalParser { toks: &toks, pos: 0, ind };
    let v = p.parse_or()?;
    Ok(bool_op(v))
}

struct EvalParser<'a> { toks: &'a [Tok], pos: usize, ind: &'a Indicators }

impl<'a> EvalParser<'a> {
    fn peek(&self) -> Option<&Tok> { self.toks.get(self.pos) }
    fn next(&mut self) -> Option<&Tok> {
        let t = self.toks.get(self.pos);
        if t.is_some() { self.pos += 1; }
        t
    }

    fn parse_or(&mut self) -> AppResult<Val> {
        let mut left = self.parse_and()?;
        while let Some(Tok::Or) = self.peek() {
            self.next();
            let right = self.parse_and()?;
            left = Val::Bool(bool_op(left) || bool_op(right));
        }
        Ok(left)
    }
    fn parse_and(&mut self) -> AppResult<Val> {
        let mut left = self.parse_not()?;
        while let Some(Tok::And) = self.peek() {
            self.next();
            let right = self.parse_not()?;
            left = Val::Bool(bool_op(left) && bool_op(right));
        }
        Ok(left)
    }
    fn parse_not(&mut self) -> AppResult<Val> {
        if let Some(Tok::Not) = self.peek() {
            self.next();
            let v = self.parse_not()?;
            return Ok(Val::Bool(!bool_op(v)));
        }
        self.parse_cmp()
    }
    fn parse_cmp(&mut self) -> AppResult<Val> {
        let left = self.parse_add()?;
        let op = match self.peek() {
            Some(t @ (Tok::Lt | Tok::Le | Tok::Gt | Tok::Ge | Tok::Eq | Tok::Ne)) => {
                let t = t.clone(); self.next(); Some(t)
            }
            _ => None,
        };
        let Some(op) = op else { return Ok(left) };
        let right = self.parse_add()?;
        let (a, b) = (num_op(left)?, num_op(right)?);
        let res = match op {
            Tok::Lt => a < b, Tok::Le => a <= b, Tok::Gt => a > b,
            Tok::Ge => a >= b, Tok::Eq => a == b, Tok::Ne => a != b,
            _ => unreachable!(),
        };
        Ok(Val::Bool(res))
    }
    fn parse_add(&mut self) -> AppResult<Val> {
        let mut left = self.parse_mul()?;
        while matches!(self.peek(), Some(Tok::Plus) | Some(Tok::Minus)) {
            let op = self.peek().cloned().unwrap(); self.next();
            let right = self.parse_mul()?;
            let (a, b) = (num_op(left)?, num_op(right)?);
            left = Val::Num(match op { Tok::Plus => a + b, _ => a - b });
        }
        Ok(left)
    }
    fn parse_mul(&mut self) -> AppResult<Val> {
        let mut left = self.parse_unary()?;
        while matches!(self.peek(), Some(Tok::Star) | Some(Tok::Slash)) {
            let op = self.peek().cloned().unwrap(); self.next();
            let right = self.parse_unary()?;
            let (a, b) = (num_op(left)?, num_op(right)?);
            left = Val::Num(match op {
                Tok::Star => a * b,
                _ => { if b == Decimal::ZERO { return Err(AppError::BadRequest("div by zero".into())); } a / b }
            });
        }
        Ok(left)
    }
    fn parse_unary(&mut self) -> AppResult<Val> {
        if let Some(Tok::Minus) = self.peek() {
            self.next();
            let v = self.parse_unary()?;
            return Ok(Val::Num(-num_op(v)?));
        }
        self.parse_atom()
    }
    fn parse_atom(&mut self) -> AppResult<Val> {
        match self.next().cloned() {
            Some(Tok::Num(n)) => Ok(Val::Num(n)),
            Some(Tok::LParen) => {
                let v = self.parse_or()?;
                match self.next() {
                    Some(Tok::RParen) => Ok(v),
                    _ => Err(AppError::BadRequest("expected ')'".into())),
                }
            }
            Some(Tok::Ident(name)) => {
                if matches!(self.peek(), Some(Tok::LParen)) {
                    self.next();
                    let mut args: Vec<Decimal> = Vec::new();
                    if !matches!(self.peek(), Some(Tok::RParen)) {
                        loop {
                            let a = num_op(self.parse_or()?)?;
                            args.push(a);
                            match self.peek() {
                                Some(Tok::Comma) => { self.next(); }
                                Some(Tok::RParen) => break,
                                _ => return Err(AppError::BadRequest("expected ',' or ')'".into())),
                            }
                        }
                    }
                    match self.next() {
                        Some(Tok::RParen) => Ok(Val::Num(resolve_fn(&name, &args, self.ind)?)),
                        _ => Err(AppError::BadRequest("expected ')'".into())),
                    }
                } else {
                    self.ind
                        .lookup(&name)
                        .map(Val::Num)
                        .ok_or_else(|| AppError::BadRequest(format!("unknown identifier: {name}")))
                }
            }
            other => Err(AppError::BadRequest(format!("unexpected token: {other:?}"))),
        }
    }
}

fn resolve_fn(name: &str, args: &[Decimal], ind: &Indicators) -> AppResult<Decimal> {
    match name {
        "rsi" => {
            let p = args.first().map(|d| d.to_string()).unwrap_or_else(|| "14".into());
            ind.rsi.get(&p.parse::<usize>().unwrap_or(14))
                .copied()
                .ok_or_else(|| AppError::BadRequest("rsi not available".into()))
        }
        "ema" => {
            let p = args.first().map(|d| d.to_string()).unwrap_or_else(|| "50".into());
            ind.ema.get(&p.parse::<usize>().unwrap_or(50))
                .copied()
                .ok_or_else(|| AppError::BadRequest("ema not available".into()))
        }
        "sma" => {
            let p = args.first().map(|d| d.to_string()).unwrap_or_else(|| "50".into());
            ind.sma.get(&p.parse::<usize>().unwrap_or(50))
                .copied()
                .ok_or_else(|| AppError::BadRequest("sma not available".into()))
        }
        "atr" => {
            let p = args.first().map(|d| d.to_string()).unwrap_or_else(|| "14".into());
            ind.atr.get(&p.parse::<usize>().unwrap_or(14))
                .copied()
                .ok_or_else(|| AppError::BadRequest("atr not available".into()))
        }
        "macd" => ind.macd.ok_or_else(|| AppError::BadRequest("macd not available".into())),
        "price" | "close" => Ok(ind.close),
        "high" => Ok(ind.high),
        "low" => Ok(ind.low),
        "open" => Ok(ind.open),
        "volume" => Ok(ind.volume),
        "pct_change" => Ok(ind.pct_change),
        "cross" | "crossup" | "crossdown" => {
            // Without historical series per-call we approximate cross as sign comparison.
            let a = args.get(0).copied().unwrap_or(Decimal::ZERO);
            let b = args.get(1).copied().unwrap_or(Decimal::ZERO);
            if name == "crossdown" {
                Ok(if a < b { Decimal::ONE } else { Decimal::ZERO })
            } else {
                Ok(if a > b { Decimal::ONE } else { Decimal::ZERO })
            }
        }
        // Candlestick patterns can be called as functions too: hammer(), doji(), etc.
        _ => {
            if let Some(v) = ind.patterns.get(name) {
                Ok(*v)
            } else {
                Err(AppError::BadRequest(format!("unknown function or pattern: {name}")))
            }
        }
    }
}

use crate::analytics::AnalyticsSummary;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct Insight {
    pub summary: AnalyticsSummary,
    pub recent_signals: usize,
    pub open_exposure: rust_decimal::Decimal,
    pub notes: Vec<String>,
}

/// Build human-readable insights atop the analytics summary.
pub fn build(summary: AnalyticsSummary, recent_signals: usize, open_exposure: rust_decimal::Decimal) -> Insight {
    let mut notes = Vec::new();

    if summary.closed_trades >= 5 {
        let wr = summary.win_rate * rust_decimal::Decimal::from(100);
        if wr > rust_decimal::Decimal::from(55) {
            notes.push(format!("Strong edge: {wr:.1}% win rate across {} trades.", summary.closed_trades));
        } else if wr < rust_decimal::Decimal::from(40) {
            notes.push(format!("Underperforming: {wr:.1}% win rate. Consider disabling weak strategies."));
        }
    }

    if summary.total_pnl < rust_decimal::Decimal::ZERO {
        notes.push("Net negative PnL — review stop-loss sizing and strategy risk_per_trade.".into());
    }

    if open_exposure > summary.total_pnl.abs() && open_exposure > rust_decimal::Decimal::ZERO {
        notes.push("Significant open exposure relative to realized PnL; watch margin.".into());
    }

    if recent_signals > 0 && summary.open_trades == 0 {
        notes.push("Signals firing but no open trades — check trading mode (paper/signals/live).".into());
    }

    if notes.is_empty() {
        notes.push("Performance looks balanced. Keep monitoring strategy decay.".into());
    }

    Insight { summary: summary.clone(), recent_signals, open_exposure, notes }
}

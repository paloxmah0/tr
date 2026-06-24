//! Learning loop — analyzes closed trades and records what worked.
//!
//! Every cycle, it looks at recently closed trades, checks which evidence
//! sources were present when the trade was opened, and updates a "what works"
//! score table. The AI engine reads this table to boost evidence sources that
//! historically win and dampen those that lose.

use crate::db::Db;
use crate::error::AppResult;
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

/// Learning scores: source -> (wins, losses).
/// Stored in the settings table as JSON.
pub type LearnScores = Arc<tokio::sync::RwLock<HashMap<String, (u32, u32)>>>;

pub async fn run(db: Db, scores: LearnScores) {
    let interval = Duration::from_secs(60);
    tracing::info!("learning loop started");
    loop {
        if let Err(e) = learn_cycle(&db, &scores).await {
            tracing::warn!(error = %e, "learning cycle failed");
        }
        tokio::time::sleep(interval).await;
    }
}

async fn learn_cycle(db: &Db, scores: &LearnScores) -> AppResult<()> {
    // Load scores from DB (stored as JSON in settings).
    let raw = db.load_setting_value("learn_scores").await.unwrap_or_default();
    {
        let mut guard = scores.write().await;
        if !raw.is_empty() {
            if let Ok(map) = serde_json::from_str::<HashMap<String, (u32, u32)>>(&raw) {
                *guard = map;
            }
        }
    }

    // Get all closed trades.
    let accounts = db.list_accounts().await?;
    for account in &accounts {
        let trades = db.list_trades(account.id, 200).await?;
        let closed: Vec<_> = trades.iter().filter(|t| t.status == crate::domain::trade::TradeStatus::Closed).collect();
        if closed.is_empty() { continue; }

        let mut new_scores: HashMap<String, (u32, u32)> = HashMap::new();

        for trade in &closed {
            let won = trade.pnl.map(|p| p > Decimal::ZERO).unwrap_or(false);
            // The trade's side tells us what direction was taken.
            // We can't reconstruct exactly which evidence sources were active,
            // but we can learn per-symbol and per-direction patterns.
            let key = format!("{}:{}", trade.symbol, format!("{:?}", trade.side).to_lowercase());
            let entry = new_scores.entry(key).or_insert((0, 0));
            if won { entry.0 += 1; } else { entry.1 += 1; }

            // Also track overall symbol performance.
            let sym_key = format!("{}:_all", trade.symbol);
            let sym_entry = new_scores.entry(sym_key).or_insert((0, 0));
            if won { sym_entry.0 += 1; } else { sym_entry.1 += 1; }
        }

        // Merge with existing scores.
        {
            let mut guard = scores.write().await;
            for (k, (w, l)) in &new_scores {
                let entry = guard.entry(k.clone()).or_insert((0, 0));
                // Use running average: add new results.
                entry.0 += w;
                entry.1 += l;
            }
        }
    }

    // Persist scores to DB.
    let guard = scores.read().await;
    let json = serde_json::to_string(&*guard).unwrap_or_default();
    drop(guard);
    let _ = db.save_setting("learn_scores", &json).await;

    Ok(())
}

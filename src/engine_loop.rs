use crate::db::Db;
use crate::engine;
use crate::error::AppResult;
use crate::execution;
use crate::market::{Broker, MarketProvider, MarketRegistry};
use rust_decimal::Decimal;
use std::sync::Arc;
use std::time::Duration;

/// Background tick loop: evaluate enabled strategies against live data,
/// emit signals, and manage open trades per account mode.
pub async fn run(db: Db, markets: Arc<MarketRegistry>, tick_secs: u64) {
    let interval = Duration::from_secs(tick_secs.max(1));
    tracing::info!(tick_secs, "engine loop started");
    loop {
        if let Err(e) = tick(&db, &markets).await {
            tracing::error!(error = %e, "engine tick failed");
        }
        tokio::time::sleep(interval).await;
    }
}

async fn tick(db: &Db, markets: &MarketRegistry) -> AppResult<()> {
    let strategies = db.list_enabled_strategies().await?;
    for strat in strategies {
        let provider: Arc<dyn MarketProvider> = markets.select(strat.asset_class).clone();

        // manage open trades first
        if let Err(e) = execution::manage_open_trades(db, provider.as_ref()).await {
            tracing::warn!(error = %e, "trade management failed");
        }

        let account = match db.get_account(strat.account_id).await? {
            Some(a) => a,
            None => continue,
        };

        let signal = engine::evaluate_strategy(db, provider.as_ref(), &strat, account.mode).await?;
        if let Some(signal) = signal {
            tracing::info!(strategy = %strat.name, symbol = %signal.symbol, strength = %signal.strength, "signal");
            let stop = strat.stop_loss.map(|p| offset_price(signal.price, signal.side, p, strat.asset_class, false));
            let tp = strat.take_profit.map(|p| offset_price(signal.price, signal.side, p, strat.asset_class, true));

            if matches!(account.mode, crate::domain::TradingMode::Live) {
                if let Some(broker) = markets.broker_for(strat.asset_class) {
                    let stake = stake_from_risk(account.balance, strat.risk_per_trade);
                    let limit = monetary_limit(account.balance, strat.risk_per_trade);
                    let _ = execution::handle_live_signal(
                        db, broker.as_ref(), &signal, stake, stop, tp, limit, limit,
                    )
                    .await;
                    continue;
                }
                tracing::warn!("live mode requested but no broker configured for asset class; falling back to simulated fill");
            }

            let _ = execution::handle_signal(
                db, provider.as_ref(), &signal, stop, tp, account.balance, strat.risk_per_trade,
            )
            .await;
        }
    }
    Ok(())
}

/// Risk amount in account currency.
fn monetary_limit(balance: Decimal, risk_per_trade: Decimal) -> Option<Decimal> {
    Some(balance * risk_per_trade)
}

/// Stake sized as the risk amount (for Deriv stake-basis contracts).
fn stake_from_risk(balance: Decimal, risk_per_trade: Decimal) -> Decimal {
    let stake = balance * risk_per_trade;
    // Deriv minimum stake is typically 0.35 USD; floor to a sane minimum.
    stake.max(Decimal::new(35, 2))
}

/// Convert a "pips/points" distance into an absolute price level.
/// `profit` true => take-profit (in favorable direction); false => stop-loss (adverse direction).
fn offset_price(
    entry: rust_decimal::Decimal,
    side: crate::domain::Side,
    pips: rust_decimal::Decimal,
    class: crate::domain::AssetClass,
    profit: bool,
) -> rust_decimal::Decimal {
    let pip_size = match class {
        crate::domain::AssetClass::Forex => rust_decimal::Decimal::new(1, 4),
        crate::domain::AssetClass::DerivIndex => rust_decimal::Decimal::ONE,
    };
    let delta = pips * pip_size;
    let favorable = match side {
        crate::domain::Side::Buy => entry + delta,
        crate::domain::Side::Sell => entry - delta,
    };
    let adverse = match side {
        crate::domain::Side::Buy => entry - delta,
        crate::domain::Side::Sell => entry + delta,
    };
    if profit { favorable } else { adverse }
}

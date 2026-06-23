use crate::db::Db;
use crate::domain::signal::Signal;
use crate::domain::trade::{Trade, TradeStatus};
use crate::domain::{OrderType, Side, TradingMode};
use crate::error::AppResult;
use crate::market::{Broker, MarketProvider, OrderRequest, Quote};
use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Sizing helper: risk a fraction of balance, sized by stop distance.
/// If no stop is known, fall back to a fixed fraction of balance converted to units.
pub fn position_size(
    balance: Decimal,
    risk_per_trade: Decimal,
    entry: Decimal,
    stop: Option<Decimal>,
) -> Decimal {
    let risk_amount = balance * risk_per_trade;
    match stop {
        Some(s) if s != Decimal::ZERO && s != entry => {
            let per_unit = (entry - s).abs();
            if per_unit == Decimal::ZERO { return Decimal::ZERO; }
            risk_amount / per_unit
        }
        _ => balance * Decimal::new(1, 2) / entry, // 1% notional default
    }
}

/// Decide what to do with a signal based on the account's mode.
/// - Paper: opens a paper trade, records PnL on close.
/// - Signals: records the signal only, no trade.
/// - Live: forwards to broker (here simulated via provider quote); records a trade.
pub async fn handle_signal(
    db: &Db,
    market: &dyn MarketProvider,
    signal: &Signal,
    stop: Option<Decimal>,
    take_profit: Option<Decimal>,
    balance: Decimal,
    risk_per_trade: Decimal,
) -> AppResult<Option<Trade>> {
    match signal.mode {
        TradingMode::Signals => {
            // Signal-only: persist the signal, no trade.
            db.insert_signal(signal).await?;
            Ok(None)
        }
        TradingMode::Paper | TradingMode::Live => {
            db.insert_signal(signal).await?;
            let entry = match market.quote(&signal.symbol).await {
                Ok(q) => fill_price(q, signal.side),
                Err(e) => {
                    if matches!(signal.mode, TradingMode::Live) {
                        tracing::warn!(error = %e, "live quote failed, falling back to signal price");
                    }
                    signal.price
                }
            };
            let size = position_size(balance, risk_per_trade, entry, stop);

            let trade = Trade {
                id: Uuid::new_v4(),
                account_id: signal.account_id,
                strategy_id: signal.strategy_id,
                signal_id: Some(signal.id),
                symbol: signal.symbol.clone(),
                side: signal.side,
                order_type: OrderType::Market,
                mode: signal.mode,
                size,
                entry_price: entry,
                exit_price: None,
                stop_loss: stop,
                take_profit,
                pnl: None,
                status: TradeStatus::Open,
                opened_at: Utc::now(),
                closed_at: None,
            };
            db.insert_trade(&trade).await?;
            tracing::info!(mode = ?signal.mode, symbol = %signal.symbol, side = ?signal.side, "trade opened");
            Ok(Some(trade))
        }
    }
}

fn fill_price(q: Quote, side: Side) -> Decimal {
    match side {
        Side::Buy => q.ask,
        Side::Sell => q.bid,
    }
}

/// Live execution: record the signal, place a real order via the broker, and
/// persist the resulting trade. Builds a unified `OrderRequest` carrying both
/// price-level SL/TP (spot brokers like OANDA) and monetary limits (Deriv).
pub async fn handle_live_signal(
    db: &Db,
    broker: &dyn Broker,
    signal: &Signal,
    stake: Decimal,
    stop_loss_price: Option<Decimal>,
    take_profit_price: Option<Decimal>,
    stop_loss_amount: Option<Decimal>,
    take_profit_amount: Option<Decimal>,
) -> AppResult<Option<Trade>> {
    db.insert_signal(signal).await?;

    // Duration only applies to contract brokers (Deriv); spot brokers ignore it.
    const DURATION_SECS: u32 = 300;
    let req = OrderRequest {
        symbol: signal.symbol.clone(),
        side: signal.side,
        stake,
        duration_secs: Some(DURATION_SECS),
        stop_loss_price,
        take_profit_price,
        stop_loss_amount,
        take_profit_amount,
    };
    let order = broker.place_order(req).await?;

    let trade = Trade {
        id: Uuid::new_v4(),
        account_id: signal.account_id,
        strategy_id: signal.strategy_id,
        signal_id: Some(signal.id),
        symbol: signal.symbol.clone(),
        side: signal.side,
        order_type: OrderType::Market,
        mode: TradingMode::Live,
        size: stake,
        entry_price: order.filled_price,
        exit_price: None,
        stop_loss: stop_loss_price,
        take_profit: take_profit_price,
        pnl: None,
        status: TradeStatus::Open,
        opened_at: Utc::now(),
        closed_at: None,
    };
    db.insert_trade(&trade).await?;
    db.adjust_balance(signal.account_id, stake - order.balance_after.abs()).await.ok();
    tracing::info!(
        mode = ?signal.mode,
        symbol = %signal.symbol,
        side = ?signal.side,
        broker_ref = %order.broker_ref,
        "live order placed"
    );
    Ok(Some(trade))
}

/// Manage open trades: close those hitting SL/TP using the latest quote.
pub async fn manage_open_trades(db: &Db, market: &dyn MarketProvider) -> AppResult<()> {
    let open = db.list_open_trades().await?;
    for t in open {
        let q = match market.quote(&t.symbol).await {
            Ok(q) => q,
            Err(e) => {
                tracing::warn!(error = %e, "quote failed during trade management");
                continue;
            }
        };
        let (mark, exit_hit, exit_price) = should_exit(&t, &q);
        if exit_hit {
            let pnl = compute_pnl(&t, exit_price);
            db.close_trade(t.id, exit_price, pnl).await?;
            db.adjust_balance(t.account_id, pnl).await?;
            tracing::info!(trade = %t.id, mark = %mark, pnl = %pnl, "trade closed");
        }
    }
    Ok(())
}

fn should_exit(t: &Trade, q: &Quote) -> (&'static str, bool, Decimal) {
    let price = match t.side {
        Side::Buy => q.bid,
        Side::Sell => q.ask,
    };
    // For buy: exit when price <= stop (loss) or >= tp (profit).
    // For sell: exit when price >= stop (loss) or <= tp (profit).
    match (t.stop_loss, t.take_profit) {
        (Some(sl), _) => match t.side {
            Side::Buy if price <= sl => return ("stop", true, price),
            Side::Sell if price >= sl => return ("stop", true, price),
            _ => {}
        },
        _ => {}
    }
    if let Some(tp) = t.take_profit {
        match t.side {
            Side::Buy if price >= tp => return ("tp", true, price),
            Side::Sell if price <= tp => return ("tp", true, price),
            _ => {}
        }
    }
    ("none", false, price)
}

pub fn compute_pnl(t: &Trade, exit: Decimal) -> Decimal {
    let per_unit = match t.side {
        Side::Buy => exit - t.entry_price,
        Side::Sell => t.entry_price - exit,
    };
    per_unit * t.size
}

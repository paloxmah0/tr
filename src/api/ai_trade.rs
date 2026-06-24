use crate::ai_engine::{AnalyzeRequest, Prediction, TradeRequest};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::State;
use axum::Json;
use rust_decimal::Decimal;

/// POST /api/analyze — AI reads the market and returns evidence-based bias.
pub async fn analyze(
    State(state): State<AppState>,
    Json(req): Json<AnalyzeRequest>,
) -> AppResult<Json<Prediction>> {
    let class = req.asset_class.unwrap_or_else(|| {
        if req.symbol.starts_with("frx") { crate::domain::AssetClass::Forex }
        else { crate::domain::AssetClass::DerivIndex }
    });
    let provider = state.markets.select(class).clone();
    let pred = crate::ai_engine::analyze(&state.db, provider.as_ref(), &state.llm, &req).await?;
    Ok(Json(pred))
}

/// POST /api/trade — User confirms direction and places a trade.
/// If the account is in LIVE mode and a broker is configured, the trade
/// goes to the real broker (Deriv). If PAPER mode, it's simulated.
/// If SIGNALS mode, only records the signal.
pub async fn place_trade(
    State(state): State<AppState>,
    Json(req): Json<TradeRequest>,
) -> AppResult<Json<serde_json::Value>> {
    use crate::domain::{OrderType, Side, TradingMode};
    use crate::domain::trade::TradeStatus;
    use crate::market::OrderRequest;
    use chrono::Utc;
    use uuid::Uuid;

    let class = req.asset_class.unwrap_or_else(|| {
        if req.symbol.starts_with("frx") { crate::domain::AssetClass::Forex }
        else { crate::domain::AssetClass::DerivIndex }
    });
    let provider = state.markets.select(class).clone();

    // Get current price from the market.
    let quote = provider.quote(&req.symbol).await?;
    let side = match req.direction.as_str() {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        _ => return Err(AppError::BadRequest("direction must be 'buy' or 'sell'".into())),
    };
    let entry = match side { Side::Buy => quote.ask, Side::Sell => quote.bid };

    // Get the account.
    let accounts = state.db.list_accounts().await?;
    let account = accounts.first().ok_or_else(|| AppError::NotFound("no account".into()))?;

    // Get a valid strategy_id for the FK constraint (use first strategy).
    let strategies = state.db.list_strategies(account.id).await?;
    let strategy_id = strategies.first().map(|s| s.id).unwrap_or(account.id);

    // Compute SL/TP using ATR.
    let candles = provider.candles(&req.symbol, 100).await?;
    let ind = crate::engine::rules::Indicators::compute(&candles)?;
    let atr = ind.atr.get(&14).copied().unwrap_or(entry * Decimal::new(5, 3));
    let pip = if req.symbol.starts_with("frx") { Decimal::new(1, 4) } else { Decimal::ONE };
    let sl_dist = atr.max(pip * Decimal::from(20));
    let tp_dist = sl_dist * Decimal::from(2);
    let (stop, tp) = match side {
        Side::Buy => (entry - sl_dist, entry + tp_dist),
        Side::Sell => (entry + sl_dist, entry - tp_dist),
    };

    let stake = req.stake.unwrap_or(account.balance * Decimal::new(1, 2)); // 1% default
    let tf_secs = req.timeframe_minutes * 60;

    // ═══ LIVE MODE: route through the broker ═══
    if account.mode == TradingMode::Live {
        if let Some(broker) = state.markets.broker_for(class) {
            let order_req = OrderRequest {
                symbol: req.symbol.clone(),
                side,
                stake,
                duration_secs: Some(tf_secs.max(15)),
                stop_loss_price: Some(stop),
                take_profit_price: Some(tp),
                stop_loss_amount: Some(stake),
                take_profit_amount: Some(stake * Decimal::from(2)),
            };

            tracing::info!(symbol = %req.symbol, direction = %req.direction, stake = %stake, "placing LIVE trade via broker");

            match broker.place_order(order_req).await {
                Ok(order) => {
                    let trade = crate::domain::trade::Trade {
                        id: Uuid::new_v4(),
                        account_id: account.id,
                        strategy_id,
                        signal_id: None,
                        symbol: req.symbol.clone(),
                        side,
                        order_type: OrderType::Market,
                        mode: TradingMode::Live,
                        size: stake,
                        entry_price: order.filled_price,
                        exit_price: None,
                        stop_loss: Some(stop),
                        take_profit: Some(tp),
                        pnl: None,
                        status: TradeStatus::Open,
                        opened_at: Utc::now(),
                        closed_at: None,
                    };
                    state.db.insert_trade(&trade).await?;

                    return Ok(Json(serde_json::json!({
                        "trade_id": trade.id,
                        "direction": req.direction,
                        "symbol": req.symbol,
                        "entry_price": order.filled_price,
                        "stop_loss": stop,
                        "take_profit": tp,
                        "stake": stake,
                        "mode": "live",
                        "broker_ref": order.broker_ref,
                        "balance_after": order.balance_after,
                        "message": format!("LIVE trade placed via broker: {} {} at {}", req.direction, req.symbol, order.filled_price)
                    })));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "live trade failed, falling back to paper mode");
                    // Fall through to paper mode below.
                }
            }
        } else {
            tracing::warn!("live mode but no broker — falling back to paper");
        }
    }

    // ═══ PAPER / SIGNALS MODE: simulated trade ═══
    let trade = crate::domain::trade::Trade {
        id: Uuid::new_v4(),
        account_id: account.id,
        strategy_id, // use account id as placeholder (FK to accounts)
        signal_id: None,
        symbol: req.symbol.clone(),
        side,
        order_type: OrderType::Market,
        mode: account.mode,
        size: stake,
        entry_price: entry,
        exit_price: None,
        stop_loss: Some(stop),
        take_profit: Some(tp),
        pnl: None,
        status: TradeStatus::Open,
        opened_at: Utc::now(),
        closed_at: None,
    };
    state.db.insert_trade(&trade).await?;

    Ok(Json(serde_json::json!({
        "trade_id": trade.id,
        "direction": req.direction,
        "symbol": req.symbol,
        "entry_price": entry,
        "stop_loss": stop,
        "take_profit": tp,
        "stake": stake,
        "mode": format!("{:?}", account.mode).to_lowercase(),
        "expiry_minutes": req.timeframe_minutes,
        "message": format!("Trade placed ({}): {} {} at {}", format!("{:?}", account.mode).to_lowercase(), req.direction, req.symbol, entry)
    })))
}

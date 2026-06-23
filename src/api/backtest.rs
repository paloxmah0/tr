use crate::backtest::{BacktestRequest, BacktestResult};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

/// Backtest a strategy: fetch historical candles from its market provider and
/// replay them through the rule engine. Returns equity curve + stats.
pub async fn run(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<BacktestRequest>,
) -> AppResult<Json<BacktestResult>> {
    let strategy = state
        .db
        .get_strategy(id)
        .await?
        .ok_or_else(|| AppError::NotFound("strategy".into()))?;
    let rules = state.db.list_rules(id).await?;
    if rules.is_empty() {
        return Err(AppError::BadRequest("strategy has no rules".into()));
    }

    let provider = state.markets.select(strategy.asset_class).clone();
    let count = req.candles.clamp(210, 5000);
    let result = crate::backtest::run_with_provider(&strategy, &rules, provider.as_ref(), &req.symbol, count).await?;
    Ok(Json(result))
}

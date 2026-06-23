use crate::domain::trade::{CloseTrade, Trade};
use crate::error::{AppError, AppResult};
use crate::execution::compute_pnl;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use uuid::Uuid;

pub async fn list(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> AppResult<Json<Vec<Trade>>> {
    Ok(Json(state.db.list_trades(account_id, 200).await?))
}

pub async fn close(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<CloseTrade>,
) -> AppResult<Json<Trade>> {
    // We need the trade to compute pnl.
    let open = state.db.list_open_trades().await?;
    let t = open.into_iter().find(|t| t.id == id).ok_or_else(|| AppError::NotFound("open trade".into()))?;
    let pnl = compute_pnl(&t, req.exit_price);
    let closed = state
        .db
        .close_trade(id, req.exit_price, pnl)
        .await?
        .ok_or_else(|| AppError::NotFound("trade".into()))?;
    state.db.adjust_balance(closed.account_id, pnl).await?;
    Ok(Json(closed))
}

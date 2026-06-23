use crate::error::AppResult;
use crate::insights;
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rust_decimal::Decimal;
use uuid::Uuid;

use crate::analytics::{AnalyticsSummary, StrategyPerf};

#[derive(Debug, serde::Serialize)]
pub struct AnalyticsResp {
    pub summary: AnalyticsSummary,
    pub per_strategy: Vec<StrategyPerf>,
}

pub async fn summary(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> AppResult<Json<AnalyticsResp>> {
    let summary = state.db.analytics_summary(account_id).await?;
    let per_strategy = state.db.per_strategy(account_id).await?;
    Ok(Json(AnalyticsResp { summary, per_strategy }))
}

pub async fn insights(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> AppResult<Json<insights::Insight>> {
    let summary = state.db.analytics_summary(account_id).await?;
    let signals = state.db.list_signals(account_id, 50).await?;
    let open_trades = state.db.list_open_trades().await?;
    let open_exposure: Decimal = open_trades
        .iter()
        .filter(|t| t.account_id == account_id)
        .map(|t| t.size * t.entry_price)
        .sum();
    Ok(Json(insights::build(summary, signals.len(), open_exposure)))
}

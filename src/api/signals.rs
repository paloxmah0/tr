use crate::domain::signal::{Signal, SignalFilter};
use crate::error::AppResult;
use crate::state::AppState;
use axum::extract::{Path, Query, State};
use axum::Json;
use uuid::Uuid;

pub async fn list(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Query(filter): Query<SignalFilter>,
) -> AppResult<Json<Vec<Signal>>> {
    let limit = filter.limit.unwrap_or(100);
    Ok(Json(state.db.list_signals(account_id, limit).await?))
}

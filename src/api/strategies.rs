use crate::domain::strategy::*;
use crate::domain::StrategySource;
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use serde::Serialize;
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub struct StrategyWithRules {
    #[serde(flatten)]
    pub strategy: Strategy,
    pub rules: Vec<Rule>,
}

fn attach_rules(s: Strategy, rules: Vec<Rule>) -> StrategyWithRules {
    StrategyWithRules { strategy: s, rules }
}

pub async fn create(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
    Json(req): Json<CreateStrategy>,
) -> AppResult<Json<StrategyWithRules>> {
    let s = state.db.create_strategy(account_id, &req, StrategySource::Manual).await?;
    let rules = state.db.list_rules(s.id).await?;
    Ok(Json(attach_rules(s, rules)))
}

pub async fn list(
    State(state): State<AppState>,
    Path(account_id): Path<Uuid>,
) -> AppResult<Json<Vec<StrategyWithRules>>> {
    let strategies = state.db.list_strategies(account_id).await?;
    let mut out = Vec::with_capacity(strategies.len());
    for s in strategies {
        let rules = state.db.list_rules(s.id).await?;
        out.push(attach_rules(s, rules));
    }
    Ok(Json(out))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<StrategyWithRules>> {
    let s = state.db.get_strategy(id).await?.ok_or_else(|| AppError::NotFound("strategy".into()))?;
    let rules = state.db.list_rules(id).await?;
    Ok(Json(attach_rules(s, rules)))
}

pub async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<UpdateStrategy>,
) -> AppResult<Json<StrategyWithRules>> {
    let s = state.db.update_strategy(id, &req).await?.ok_or_else(|| AppError::NotFound("strategy".into()))?;
    let rules = state.db.list_rules(id).await?;
    Ok(Json(attach_rules(s, rules)))
}

pub async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let ok = state.db.delete_strategy(id).await?;
    Ok(Json(serde_json::json!({ "deleted": ok })))
}

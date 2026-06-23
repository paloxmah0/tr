use crate::domain::{TradingMode};
use crate::error::{AppError, AppResult};
use crate::state::AppState;
use axum::extract::{Path, State};
use axum::Json;
use rust_decimal::Decimal;
use serde::Deserialize;
use uuid::Uuid;

use crate::domain::Account;

#[derive(Debug, Deserialize)]
pub struct CreateAccount {
    pub label: String,
    pub broker: String,
    pub account_ref: String,
    #[serde(default)]
    pub balance: Decimal,
    #[serde(default = "usd")]
    pub currency: String,
    #[serde(default)]
    pub mode: Option<String>,
}

fn usd() -> String { "USD".into() }

pub async fn create(
    State(state): State<AppState>,
    Json(req): Json<CreateAccount>,
) -> AppResult<Json<Account>> {
    let mode = req
        .mode
        .as_deref()
        .and_then(TradingMode::parse)
        .unwrap_or_else(|| {
            TradingMode::parse(&state.settings.default_trading_mode).unwrap_or(TradingMode::Paper)
        });
    let acc = state
        .db
        .create_account(&req.label, &req.broker, &req.account_ref, req.balance, &req.currency, mode)
        .await?;
    Ok(Json(acc))
}

pub async fn list(State(state): State<AppState>) -> AppResult<Json<Vec<Account>>> {
    Ok(Json(state.db.list_accounts().await?))
}

pub async fn get(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<Account>> {
    state
        .db
        .get_account(id)
        .await?
        .ok_or_else(|| AppError::NotFound("account".into()))
        .map(Json)
}

#[derive(Debug, Deserialize)]
pub struct SetMode {
    pub mode: String,
}

pub async fn set_mode(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<SetMode>,
) -> AppResult<Json<Account>> {
    let mode = TradingMode::parse(&req.mode)
        .ok_or_else(|| AppError::BadRequest("invalid mode (paper|signals|live)".into()))?;
    state
        .db
        .set_account_mode(id, mode)
        .await?
        .ok_or_else(|| AppError::NotFound("account".into()))
        .map(Json)
}

use crate::db::Db;
use crate::domain::strategy::*;
use crate::domain::{AssetClass, StrategySource};
use crate::error::{AppError, AppResult};
use sqlx::Row;
use uuid::Uuid;

impl Db {
    pub async fn create_strategy(
        &self,
        account_id: Uuid,
        req: &CreateStrategy,
        source: StrategySource,
    ) -> AppResult<Strategy> {
        let mut tx = self.pool.begin().await?;
        let symbols = serde_json::to_value(&req.symbols)?;

        let row = sqlx::query(
            r#"INSERT INTO strategies (account_id, name, description, asset_class, symbols, stop_loss, take_profit, risk_per_trade, source)
               VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
               RETURNING id, account_id, name, description, asset_class, symbols, stop_loss, take_profit,
                         risk_per_trade, enabled, source, created_at, updated_at"#,
        )
        .bind(account_id)
        .bind(&req.name)
        .bind(req.description.as_ref())
        .bind(req.asset_class as AssetClass)
        .bind(symbols)
        .bind(req.stop_loss)
        .bind(req.take_profit)
        .bind(req.risk_per_trade)
        .bind(source as StrategySource)
        .fetch_one(&mut *tx)
        .await?;

        let strategy_id: Uuid = row.get("id");

        for r in &req.rules {
            sqlx::query(
                r#"INSERT INTO rules (strategy_id, name, expr, weight, enabled) VALUES ($1, $2, $3, $4, true)"#,
            )
            .bind(strategy_id)
            .bind(&r.name)
            .bind(&r.expr)
            .bind(r.weight)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        self.get_strategy(strategy_id)
            .await?
            .ok_or_else(|| AppError::Internal("strategy not found after insert".into()))
    }

    pub async fn get_strategy(&self, id: Uuid) -> AppResult<Option<Strategy>> {
        let row = sqlx::query(
            r#"SELECT id, account_id, name, description, asset_class, symbols, stop_loss, take_profit,
                      risk_per_trade, enabled, source, created_at, updated_at
               FROM strategies WHERE id = $1"#,
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.as_ref().map(map_strategy))
    }

    pub async fn list_strategies(&self, account_id: Uuid) -> AppResult<Vec<Strategy>> {
        let rows = sqlx::query(
            r#"SELECT id, account_id, name, description, asset_class, symbols, stop_loss, take_profit,
                      risk_per_trade, enabled, source, created_at, updated_at
               FROM strategies WHERE account_id = $1 ORDER BY created_at DESC"#,
        )
        .bind(account_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(map_strategy).collect())
    }

    pub async fn list_enabled_strategies(&self) -> AppResult<Vec<Strategy>> {
        let rows = sqlx::query(
            r#"SELECT id, account_id, name, description, asset_class, symbols, stop_loss, take_profit,
                      risk_per_trade, enabled, source, created_at, updated_at
               FROM strategies WHERE enabled = true ORDER BY created_at DESC"#,
        )
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.iter().map(map_strategy).collect())
    }

    pub async fn list_rules(&self, strategy_id: Uuid) -> AppResult<Vec<Rule>> {
        let rows = sqlx::query(
            r#"SELECT id, strategy_id, name, expr, weight, enabled FROM rules WHERE strategy_id = $1"#,
        )
        .bind(strategy_id)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows
            .iter()
            .map(|r| Rule {
                id: r.get("id"),
                strategy_id: r.get("strategy_id"),
                name: r.get("name"),
                expr: r.get("expr"),
                weight: r.get("weight"),
                enabled: r.get("enabled"),
            })
            .collect())
    }

    pub async fn update_strategy(&self, id: Uuid, req: &UpdateStrategy) -> AppResult<Option<Strategy>> {
        let symbols_json = match &req.symbols {
            Some(s) => Some(serde_json::to_value(s)?),
            None => None,
        };
        sqlx::query(
            r#"UPDATE strategies SET
                 name = COALESCE($2, name),
                 description = COALESCE($3, description),
                 symbols = COALESCE($4, symbols),
                 stop_loss = COALESCE($5, stop_loss),
                 take_profit = COALESCE($6, take_profit),
                 risk_per_trade = COALESCE($7, risk_per_trade),
                 enabled = COALESCE($8, enabled),
                 updated_at = now()
               WHERE id = $1"#,
        )
        .bind(id)
        .bind(req.name.as_ref())
        .bind(req.description.as_ref())
        .bind(symbols_json)
        .bind(req.stop_loss)
        .bind(req.take_profit)
        .bind(req.risk_per_trade)
        .bind(req.enabled)
        .execute(&self.pool)
        .await?;
        self.get_strategy(id).await
    }

    pub async fn delete_strategy(&self, id: Uuid) -> AppResult<bool> {
        let res = sqlx::query("DELETE FROM strategies WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(res.rows_affected() > 0)
    }
}

fn map_strategy(row: &sqlx::postgres::PgRow) -> Strategy {
    let symbols_val: serde_json::Value = row.get("symbols");
    let symbols: Vec<String> = serde_json::from_value(symbols_val).unwrap_or_default();
    Strategy {
        id: row.get("id"),
        account_id: row.get("account_id"),
        name: row.get("name"),
        description: row.get("description"),
        asset_class: row.get("asset_class"),
        symbols,
        stop_loss: row.get("stop_loss"),
        take_profit: row.get("take_profit"),
        risk_per_trade: row.get("risk_per_trade"),
        enabled: row.get("enabled"),
        source: row.get("source"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

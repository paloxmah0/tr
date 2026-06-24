use crate::db::Db;
use crate::error::AppResult;
use sqlx::Row;

impl Db {
    pub async fn load_settings(&self) -> AppResult<Vec<(String, String)>> {
        let rows = sqlx::query("SELECT key, value FROM settings")
            .fetch_all(&self.pool)
            .await?;
        Ok(rows.iter().map(|r| (r.get("key"), r.get("value"))).collect())
    }

    pub async fn load_setting_value(&self, key: &str) -> AppResult<String> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")).unwrap_or_default())
    }

    pub async fn save_setting(&self, key: &str, value: &str) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO settings (key, value) VALUES ($1, $2)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

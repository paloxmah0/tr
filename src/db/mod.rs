use sqlx::PgPool;

pub mod accounts;
pub mod analytics;
pub mod notes;
pub mod settings;
pub mod strategies;
pub mod trades;

#[derive(Clone)]
pub struct Db {
    pub pool: PgPool,
}

impl Db {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn run_migrations(&self) -> anyhow::Result<()> {
        sqlx::migrate!("./migrations").run(&self.pool).await?;
        Ok(())
    }
}

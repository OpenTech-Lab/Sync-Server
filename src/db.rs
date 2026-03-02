use anyhow::Result;
use diesel::r2d2::{self, ConnectionManager};
use diesel::PgConnection;
use diesel_migrations::{embed_migrations, EmbeddedMigrations, MigrationHarness};

pub type Pool = r2d2::Pool<ConnectionManager<PgConnection>>;
pub type DbConn = r2d2::PooledConnection<ConnectionManager<PgConnection>>;

pub const MIGRATIONS: EmbeddedMigrations = embed_migrations!("migrations");

pub fn create_pool(database_url: &str) -> Result<Pool> {
    let manager = ConnectionManager::<PgConnection>::new(database_url);
    let pool = r2d2::Pool::builder()
        .max_size(15)
        .build(manager)?;
    tracing::info!("Database connection pool established (max_size=15)");
    Ok(pool)
}

pub fn run_migrations(pool: &Pool) -> Result<()> {
    let mut conn = pool.get()?;
    tracing::info!("Running pending database migrations");
    conn.run_pending_migrations(MIGRATIONS)
        .map_err(|e| anyhow::anyhow!("Migration failed: {}", e))?;
    tracing::info!("Database migrations complete");
    Ok(())
}

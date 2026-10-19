//! Database connection setup and migration execution.
pub(in crate::dashboard) mod entities;
mod migrations;

use super::error::Result;
use sea_orm::{ConnectOptions, Database, DatabaseConnection};
use sea_orm_migration::MigratorTrait;
use std::time::Duration;
use tracing::instrument::WithSubscriber;

/// Covers concurrent requests plus the background workers; every SQL transaction is short
/// because password and WebAuthn CPU work runs before any transaction or row lock.
const MAX_CONNECTIONS: u32 = 16;

pub(super) async fn connect(url: &str) -> Result<DatabaseConnection> {
    let mut options = ConnectOptions::new(url);
    options
        .max_connections(MAX_CONNECTIONS)
        .connect_timeout(Duration::from_secs(10))
        .sqlx_logging(false);
    // SeaORM's connection TRACE span captures options, including the database URL.
    Ok(Database::connect(options)
        .with_subscriber(tracing::subscriber::NoSubscriber::default())
        .await?)
}

pub(super) async fn migrate(db: &DatabaseConnection) -> Result<()> {
    migrations::Migrator::up(db, None).await?;
    Ok(())
}

#[cfg(test)]
mod game_tests;
#[cfg(test)]
mod tests;

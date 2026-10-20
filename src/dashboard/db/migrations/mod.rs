//! Ordered migration registry; names and schema live in their individual migrations.
mod account_activity;
mod dashboard_games;
mod hoyolab_accounts;
mod hoyolab_cookie_removal;
mod initial_accounts;
mod passkeys;

use sea_orm_migration::prelude::*;

pub(super) struct Migrator;

#[async_trait::async_trait]
impl MigratorTrait for Migrator {
    fn migrations() -> Vec<Box<dyn MigrationTrait>> {
        vec![
            Box::new(initial_accounts::InitialAccounts),
            Box::new(passkeys::Passkeys),
            Box::new(hoyolab_accounts::HoyolabAccounts),
            Box::new(dashboard_games::DashboardGames),
            Box::new(hoyolab_cookie_removal::HoyolabCookieRemoval),
            Box::new(account_activity::AccountActivity),
        ]
    }
}

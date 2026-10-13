//! Command dispatch and bounded standard-input handling; no account policy here.
use super::{
    auth::accounts::Accounts,
    config::Config,
    db,
    error::{Error, Result},
    http,
};
use std::io::Read;

pub(crate) async fn run() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("serve");
    if command == "help" || command == "--help" {
        println!(
            "Commands: serve | migrate | bootstrap-admin USERNAME\nbootstrap-admin reads the temporary password from standard input"
        );
        return Ok(());
    }
    let valid = matches!(
        (command, args.len()),
        ("serve" | "migrate", 0 | 1) | ("bootstrap-admin", 2)
    );
    if !valid {
        return Err(Error::Invalid(
            "usage: attosgameapi [serve | migrate | bootstrap-admin USERNAME]",
        ));
    }
    let config = Config::load()?;
    let db = db::connect(&config.database_url).await?;
    match command {
        "serve" => http::serve(db, config).await?,
        "migrate" => {
            db::migrate(&db).await?;
            println!("Migrations applied");
        }
        "bootstrap-admin" => {
            let username = args[1].clone();
            let password = tokio::task::spawn_blocking(read_password)
                .await
                .map_err(|_| Error::Internal)??;
            let accounts = Accounts::new(db.clone(), config.session_ttl).await?;
            let user = accounts.bootstrap_admin(username, password).await?;
            println!(
                "Administrator created: {} (password change required)",
                user.id
            );
        }
        _ => return Err(Error::Invalid("unknown command")),
    }
    Ok(())
}

fn read_password() -> Result<String> {
    read_password_from(std::io::stdin())
}

fn read_password_from(input: impl Read) -> Result<String> {
    let mut password = String::new();
    input
        .take(259)
        .read_to_string(&mut password)
        .map_err(|_| Error::Invalid("could not read password from standard input"))?;
    if password.len() >= 259 {
        return Err(Error::Invalid("password input is too long"));
    }
    if password.ends_with('\n') {
        password.pop();
        if password.ends_with('\r') {
            password.pop();
        }
    }
    Ok(password)
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;

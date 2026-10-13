//! Explicit opt-in checks of the current read-only upstream endpoints.
//! Never signs rewards, changes privacy settings, or prints credentials/payloads.
use super::*;
use crate::game::{Game, Genshin, Starrail};

#[tokio::test]
#[ignore = "real HoYoLAB GET requests require explicit approval and HOYOLAB_COOKIE"]
async fn current_read_only_endpoints() {
    let cookie = std::env::var("HOYOLAB_COOKIE")
        .unwrap_or_else(|_| panic!("set HOYOLAB_COOKIE explicitly; no dotenv file is read"));
    let user_agent = match std::env::var("HOYOLAB_USER_AGENT") {
        Ok(value) if !value.is_empty() => value,
        Ok(_) | Err(std::env::VarError::NotPresent) => UA.to_owned(),
        Err(_) => panic!("HOYOLAB_USER_AGENT must be valid Unicode"),
    };
    let client = HoyolabClient::with_user_agent(&cookie, &user_agent).unwrap();
    tokio::time::timeout(Duration::from_secs(90), async {
        let accounts = client.accounts().await?;
        check_game(&client, Genshin, &accounts, "hk4e_global").await?;
        check_game(&client, Starrail, &accounts, "hkrpg_global").await
    })
    .await
    .expect("read-only API checks exceeded 90 seconds")
    .unwrap();
}

async fn check_game<G: Game + Copy>(
    client: &HoyolabClient,
    game: G,
    accounts: &[Account],
    biz: &str,
) -> Result<()> {
    client.daily_info(game).await?;
    client.daily_home(game).await?;
    // Keep the first account on level ties, as in the former diagnostic CLI.
    let account = accounts
        .iter()
        .filter(|account| account.game_biz == biz)
        .reduce(|a, b| if b.level > a.level { b } else { a })
        .ok_or_else(|| {
            Error::InvalidInput("required overseas game account is not linked".into())
        })?;
    let account = RecordAccount {
        uid: account.game_uid.clone(),
        server: account.region.clone(),
    };
    client.index(game, &account).await?;
    client.daily_note(game, &account).await?;
    Ok(())
}

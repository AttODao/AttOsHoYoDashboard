// Read-only live check. Never claims rewards or changes privacy settings.
use attosgameapi::{
    game::{Game, Genshin, Starrail},
    hoyolab::{HoyolabClient, RecordAccount, Result},
    models::account::Account,
};
use serde::Serialize;
use std::collections::HashMap;

#[tokio::main]
async fn main() {
    let mut env_path = ".env".to_owned();
    let mut raw = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--raw" | "-raw" => raw = true,
            "--env" | "-env" => match args.next() {
                Some(path) => env_path = path,
                None => fail("--env にはファイルパスが必要です"),
            },
            "--help" | "-h" => {
                println!("cargo run --bin hoyolab_check -- [--env .env] [--raw]");
                return;
            }
            _ => fail("不明な引数です。--help を参照してください"),
        }
    }
    match tokio::time::timeout(std::time::Duration::from_secs(90), run(&env_path, raw)).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => fail(&error.to_string()),
        Err(_) => fail("全体の制限時間90秒を超えました"),
    }
}

async fn run(env_path: &str, raw: bool) -> Result<()> {
    let config = load_config(env_path)?;
    let get = |key: &str| config.get(key).map(|value| value.trim()).unwrap_or("");
    let mut cookie = get("HOYOLAB_COOKIE").to_owned();
    if cookie.is_empty()
        && !get("HOYOLAB_LTUID_V2").is_empty()
        && !get("HOYOLAB_LTOKEN_V2").is_empty()
    {
        cookie = format!(
            "ltuid_v2={}; ltoken_v2={}",
            get("HOYOLAB_LTUID_V2"),
            get("HOYOLAB_LTOKEN_V2")
        );
    }
    let client = if get("HOYOLAB_USER_AGENT").is_empty() {
        HoyolabClient::new(&cookie)?
    } else {
        HoyolabClient::with_user_agent(&cookie, get("HOYOLAB_USER_AGENT"))?
    };
    let accounts = client.accounts().await?;
    println!("OK   Cookie認証・連携アカウント一覧（個人情報は非表示）");
    let genshin = check_game(&client, Genshin, &accounts, "hk4e_global", "原神", raw).await;
    let starrail = check_game(
        &client,
        Starrail,
        &accounts,
        "hkrpg_global",
        "スターレイル",
        raw,
    )
    .await;
    if !genshin || !starrail {
        return Err("一部APIの確認に失敗しました（上記の結果を確認してください）".into());
    }
    println!("OK   読み取りAPIの通信・モデル変換を確認しました（signは未実行）");
    Ok(())
}

async fn check_game<G: Game + Copy>(
    client: &HoyolabClient,
    game: G,
    accounts: &[Account],
    biz: &str,
    label: &str,
    raw: bool,
) -> bool {
    println!("\n[{label}]");
    let info = show("daily/info", client.daily_info(game).await, raw);
    let home = show("daily/home", client.daily_home(game).await, raw);
    let Some(account) = select_account(accounts, biz) else {
        println!("SKIP index/dailyNote: 連携済み海外版アカウントがありません");
        return false;
    };
    let account = RecordAccount {
        uid: account.game_uid.clone(),
        server: account.region.clone(),
    };
    let index = show("index", client.index(game, &account).await, raw);
    let note = show("dailyNote", client.daily_note(game, &account).await, raw);
    info && home && index && note
}

fn show<T: Serialize>(label: &str, result: Result<T>, raw: bool) -> bool {
    match result {
        Ok(data) => {
            println!("OK   {label}: 型付きモデルへの変換成功");
            if raw {
                match serde_json::to_string_pretty(&data) {
                    Ok(json) => println!("{json}"),
                    Err(_) => {
                        println!("FAIL {label}: JSONの整形に失敗しました");
                        return false;
                    }
                }
            }
            true
        }
        Err(error) => {
            println!("FAIL {label}: {error}");
            false
        }
    }
}

fn select_account<'a>(accounts: &'a [Account], biz: &str) -> Option<&'a Account> {
    // Keep the first account on ties, matching the original client.
    accounts
        .iter()
        .filter(|account| account.game_biz == biz)
        .reduce(|a, b| if b.level > a.level { b } else { a })
}

fn load_config(path: &str) -> Result<HashMap<String, String>> {
    let entries = dotenvy::from_path_iter(path).map_err(|_| {
        format!("{path} を開けません。.env.example をコピーしてCookieを設定してください")
    })?;
    let mut config: HashMap<String, String> = std::env::vars().collect();
    for entry in entries {
        // Never print dotenv parser errors: they may contain the secret line.
        let (key, value) = entry.map_err(|_| format!("{path}: KEY=VALUE 形式が不正です"))?;
        config.entry(key).or_insert(value);
    }
    Ok(config)
}

fn fail(message: &str) -> ! {
    eprintln!("ERROR: {message}");
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dotenv_precedence_and_redaction() {
        #[derive(serde::Deserialize)]
        struct Accounts {
            list: Vec<Account>,
        }
        let accounts: Accounts = serde_json::from_str(r#"{"list":[{"game_biz":"hk4e_global","region":"os_asia","game_uid":"1","level":20},{"game_biz":"hk4e_global","region":"os_usa","game_uid":"2","level":30},{"game_biz":"hk4e_global","region":"os_euro","game_uid":"3","level":30}]}"#).unwrap();
        assert_eq!(
            select_account(&accounts.list, "hk4e_global")
                .unwrap()
                .game_uid,
            "2"
        );
        assert!(select_account(&accounts.list, "hkrpg_global").is_none());
        let path = std::env::temp_dir().join(format!(
            "attos-dotenv-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ));
        std::fs::write(
            &path,
            "PATH=do-not-override\nexport ATTOS_CONFIG_TEST='quoted value'\n",
        )
        .unwrap();
        let config = load_config(path.to_str().unwrap()).unwrap();
        assert_eq!(config["PATH"], std::env::var("PATH").unwrap());
        assert_eq!(config["ATTOS_CONFIG_TEST"], "quoted value");
        std::fs::write(&path, "HOYOLAB_COOKIE=\"secret-unclosed\n").unwrap();
        let error = load_config(path.to_str().unwrap()).unwrap_err();
        assert!(!error.to_string().contains("secret-unclosed"));
        std::fs::remove_file(path).unwrap();
    }
}

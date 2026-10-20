//! Explicitly approved, read-only expedition inspection of the local development account.
//! Configuration/credentials stay in process memory; output contains only counts and safe fields.
use super::summary::summarize;
use crate::dashboard::{
    config::Config,
    db::entities::{game_account, hoyolab_account, user},
    hoyolab::gateway::{self, Gateway, LiveGateway},
};
use attoshoyodashboard::hoyolab::RecordAccount;
use chrono::Utc;
use sea_orm::{
    ColumnTrait, ConnectOptions, ConnectionTrait, Database, EntityTrait, QueryFilter, QuerySelect,
    TransactionTrait,
};
use serde_json::{Value, json};
use std::{env, time::Duration};

#[tokio::test]
#[ignore = "uses a stored local-development Cookie for real HoYoLAB GETs; requires separate explicit approval"]
async fn approved_stored_cookie_expeditions() {
    assert_eq!(
        env::var("ATTOS_APPROVED_LIVE_EXPEDITIONS").ok().as_deref(),
        Some("1"),
        "explicit expedition approval flag is required"
    );
    assert!(
        env::var_os("DATABASE_URL").is_none(),
        "DATABASE_URL must not be supplied"
    );
    let runtime = env::var("DEVENV_RUNTIME").expect("local devenv runtime is required");
    assert_eq!(
        env::var("DB_HOST").ok(),
        Some(format!("{runtime}/postgres")),
        "only the local devenv PostgreSQL socket is allowed"
    );
    let config = Config::load().unwrap_or_else(|_| panic!("local diagnostic configuration failed"));
    let key = config
        .cookie_key
        .expect("local Cookie encryption key is required");
    let mut options = ConnectOptions::new(config.database_url);
    options.max_connections(1).sqlx_logging(false);
    let db = Database::connect(options)
        .await
        .unwrap_or_else(|_| panic!("local diagnostic DB connection failed"));
    let tx = db
        .begin()
        .await
        .unwrap_or_else(|_| panic!("read-only transaction failed"));
    tx.execute_unprepared("SET TRANSACTION READ ONLY")
        .await
        .unwrap_or_else(|_| panic!("could not enforce read-only diagnostics"));
    let links = hoyolab_account::Entity::find()
        .filter(hoyolab_account::Column::AuthStatus.eq("verified"))
        .all(&tx)
        .await
        .unwrap_or_else(|_| panic!("stored link lookup failed"));
    assert_eq!(
        links.len(),
        1,
        "exactly one verified link is required; select an owner explicitly otherwise"
    );
    let link = &links[0];
    let active = user::Entity::find_by_id(link.owner_user_id)
        .select_only()
        .column(user::Column::Active)
        .into_tuple::<bool>()
        .one(&tx)
        .await
        .unwrap_or_else(|_| panic!("owner readiness lookup failed"));
    assert_eq!(active, Some(true), "stored link owner must be active");
    let cookie = key
        .open(
            link.owner_user_id,
            link.hoyolab_uid.as_deref(),
            link.sealed_credentials().unwrap(),
        )
        .unwrap_or_else(|_| panic!("stored Cookie decryption failed"));
    let accounts = game_account::Entity::find()
        .filter(game_account::Column::HoyolabAccountId.eq(link.id))
        .filter(game_account::Column::Detected.eq(true))
        .all(&tx)
        .await
        .unwrap_or_else(|_| panic!("stored game lookup failed"));
    assert!(
        !accounts.is_empty() && accounts.len() <= 6,
        "expected one to six detected game accounts"
    );
    tx.rollback()
        .await
        .unwrap_or_else(|_| panic!("read-only transaction close failed"));
    let gateway = LiveGateway;
    for account in accounts {
        let game =
            gateway::game(&account.game_kind).unwrap_or_else(|_| panic!("unsupported stored game"));
        let record = RecordAccount {
            uid: account.game_uid,
            server: account.region,
        };
        let raw = tokio::time::timeout(
            Duration::from_secs(30),
            gateway.note(&cookie, game, &record),
        )
        .await
        .expect("read-only note GET timed out")
        .unwrap_or_else(|error| {
            panic!(
                "{} note GET failed: {}",
                account.game_kind,
                gateway::error_code(&error)
            )
        });
        let summary =
            summarize(game, Utc::now(), &raw).expect("note GET returned no summary fields");
        let expeditions = raw
            .get("expeditions")
            .and_then(Value::as_array)
            .expect("note expedition list is missing");
        let details: Vec<Value> = expeditions.iter().map(|entry| {
            let icon = entry.get(if account.game_kind == "genshin" { "avatar_side_icon" } else { "item_url" }).and_then(Value::as_str);
            let host = icon.and_then(|url| reqwest::Url::parse(url).ok()).and_then(|url| url.host_str().map(str::to_owned));
            let seconds = entry.get("remaining_time").or_else(|| entry.get("remained_time"));
            json!({
                "status": match entry.get("status").and_then(Value::as_str) { Some("Ongoing") => "ongoing", Some("Finished") => "finished", _ => "other" },
                "remaining_seconds": seconds.and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse::<u64>().ok())),
                "icon_present": icon.is_some_and(|icon| !icon.is_empty()),
                "official_icon_host": host.as_deref().filter(|host| [".hoyolab.com", ".hoyoverse.com", ".mihoyo.com"].iter().any(|suffix| host.ends_with(suffix))),
                "icon_csp_allowed": matches!(host.as_deref(), Some("upload-os-bbs.hoyolab.com" | "act-upload.hoyolab.com" | "act-webstatic.hoyoverse.com")),
            })
        }).collect();
        let counts: serde_json::Map<String, Value> = [
            "current_expedition_num",
            "max_expedition_num",
            "accepted_epedition_num",
            "total_expedition_num",
        ]
        .into_iter()
        .filter_map(|key| {
            raw.get(key)
                .and_then(Value::as_u64)
                .map(|count| (key.to_owned(), json!(count)))
        })
        .collect();
        println!(
            "LIVE_EXPEDITIONS {}",
            json!({
                "game": account.game_kind,
                "count": expeditions.len(),
                "reported_counts": counts,
                "summary_count": summary.get("expeditions").and_then(Value::as_array).map(Vec::len),
                "entries": details,
            })
        );
    }
    db.close()
        .await
        .unwrap_or_else(|_| panic!("local diagnostic DB close failed"));
}

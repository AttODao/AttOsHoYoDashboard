//! Prometheus text metrics, served only with `METRICS_TOKEN` as a bearer token.
use super::{App, Error, Result};
use axum::{
    extract::State,
    http::{HeaderMap, header},
    response::IntoResponse,
};
use sea_orm::{ConnectionTrait, DbBackend, Statement};
use sha2::{Digest, Sha256};
use std::fmt::Write;

/// Counts by label from fixed SQL; label values come from CHECK-constrained columns.
const GROUPS: [(&str, &str, &str); 5] = [
    (
        "ahd_users",
        "state",
        "SELECT CASE WHEN active THEN 'active' ELSE 'inactive' END AS label, count(*) AS n FROM users GROUP BY 1",
    ),
    (
        "ahd_hoyolab_links",
        "status",
        "SELECT auth_status AS label, count(*) AS n FROM hoyolab_accounts GROUP BY 1",
    ),
    (
        "ahd_reward_runs",
        "state",
        "SELECT state AS label, count(*) AS n FROM reward_runs GROUP BY 1",
    ),
    (
        "ahd_reward_settings",
        "automatic",
        "SELECT CASE WHEN enabled THEN 'on' ELSE 'off' END AS label, count(*) AS n FROM reward_settings GROUP BY 1",
    ),
    (
        "ahd_api_tokens",
        "scope",
        "SELECT scope AS label, count(*) AS n FROM api_tokens GROUP BY 1",
    ),
];

/// Final reward run states; every one is reported so an absent series never means "none".
const ENDED_STATES: [&str; 6] = [
    "success",
    "already_claimed",
    "failed",
    "action_required",
    "unknown",
    "stopped",
];
const APP_DEVICES: &str = "SELECT CASE WHEN push_subscription IS NULL THEN 'poll' ELSE 'push' END AS label, count(*) AS n FROM app_devices GROUP BY 1";
const CODES: &str = "SELECT game_kind || ':' || CASE WHEN listed THEN 'listed' ELSE 'unlisted' END AS label, count(*) AS n FROM redemption_codes GROUP BY 1";
const LEDGER: &str = "SELECT state AS label, count(*) AS n FROM redemption_ledger GROUP BY 1";
const ENDED_24H: &str = "SELECT state AS label, count(*) AS n FROM reward_runs WHERE ended_at >= now() - interval '24 hours' GROUP BY 1";

pub(super) async fn metrics(
    State(app): State<App>,
    headers: HeaderMap,
) -> Result<impl IntoResponse> {
    // Without a configured token the endpoint does not exist.
    let expected = app.config.metrics_token.as_deref().ok_or(Error::NotFound)?;
    let presented = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .unwrap_or_default();
    // Compare digests so the comparison time does not depend on the token's content.
    if Sha256::digest(presented.as_bytes()) != Sha256::digest(expected.as_bytes()) {
        return Err(Error::Unauthorized);
    }
    let mut out = String::new();
    let _ = writeln!(out, "# TYPE ahd_up gauge\nahd_up 1");
    let _ = writeln!(
        out,
        "# HELP ahd_reward_worker_heartbeat_age_seconds Seconds since the reward worker last ticked.\n# TYPE ahd_reward_worker_heartbeat_age_seconds gauge\nahd_reward_worker_heartbeat_age_seconds {}",
        app.heartbeat.age_secs().max(0)
    );
    for (name, label, sql) in GROUPS {
        let rows = app
            .accounts
            .db
            .query_all_raw(Statement::from_string(DbBackend::Postgres, sql))
            .await?;
        let _ = writeln!(out, "# TYPE {name} gauge");
        for row in rows {
            let value: String = row.try_get("", "label")?;
            let count: i64 = row.try_get("", "n")?;
            let _ = writeln!(out, "{name}{{{label}=\"{value}\"}} {count}");
        }
    }
    let rows = app
        .accounts
        .db
        .query_all_raw(Statement::from_string(DbBackend::Postgres, ENDED_24H))
        .await?;
    let mut ended = std::collections::BTreeMap::new();
    for row in rows {
        let state: String = row.try_get("", "label")?;
        let count: i64 = row.try_get("", "n")?;
        ended.insert(state, count);
    }
    let _ = writeln!(
        out,
        "# HELP ahd_reward_runs_ended_24h Reward runs that ended in the last 24 hours, by final state.\n# TYPE ahd_reward_runs_ended_24h gauge"
    );
    for state in ENDED_STATES {
        let count = ended.get(state).copied().unwrap_or(0);
        let _ = writeln!(
            out,
            "ahd_reward_runs_ended_24h{{state=\"{state}\"}} {count}"
        );
    }
    let rows = app
        .accounts
        .db
        .query_all_raw(Statement::from_string(DbBackend::Postgres, APP_DEVICES))
        .await?;
    let mut devices = std::collections::BTreeMap::new();
    for row in rows {
        let delivery: String = row.try_get("", "label")?;
        let count: i64 = row.try_get("", "n")?;
        devices.insert(delivery, count);
    }
    let _ = writeln!(
        out,
        "# HELP ahd_app_devices Registered app installations, by how they learn of new notices.\n# TYPE ahd_app_devices gauge"
    );
    for delivery in ["push", "poll"] {
        let count = devices.get(delivery).copied().unwrap_or(0);
        let _ = writeln!(out, "ahd_app_devices{{delivery=\"{delivery}\"}} {count}");
    }
    let _ = writeln!(
        out,
        "# HELP ahd_app_push_requests_total App push requests since start, by outcome (gone: the subscription was removed).\n# TYPE ahd_app_push_requests_total counter"
    );
    for (outcome, count) in app.notifications.push_outcomes() {
        let _ = writeln!(
            out,
            "ahd_app_push_requests_total{{outcome=\"{outcome}\"}} {count}"
        );
    }
    codes(&app, &mut out).await?;
    Ok((
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    ))
}

/// Redemption-code catalogue size, fetch outcomes, rejected rows and announcements.
async fn codes(app: &App, out: &mut String) -> Result<()> {
    let rows = app
        .accounts
        .db
        .query_all_raw(Statement::from_string(DbBackend::Postgres, CODES))
        .await?;
    let mut sizes = std::collections::BTreeMap::new();
    for row in rows {
        let label: String = row.try_get("", "label")?;
        let count: i64 = row.try_get("", "n")?;
        sizes.insert(label, count);
    }
    let _ = writeln!(
        out,
        "# HELP ahd_codes_catalogue Redemption codes in the shared catalogue, by game and whether the source still lists them.\n# TYPE ahd_codes_catalogue gauge"
    );
    let stats = &app.codes.stats;
    for game in crate::dashboard::codes::catalog::GAMES {
        for state in ["listed", "unlisted"] {
            let count = sizes.get(&format!("{game}:{state}")).copied().unwrap_or(0);
            let _ = writeln!(
                out,
                "ahd_codes_catalogue{{game=\"{game}\",state=\"{state}\"}} {count}"
            );
        }
    }
    let _ = writeln!(
        out,
        "# HELP ahd_codes_fetches_total Code source fetches since start, by game and outcome (ok or an error class).\n# TYPE ahd_codes_fetches_total counter"
    );
    for (game, outcome, count) in stats.fetches() {
        let _ = writeln!(
            out,
            "ahd_codes_fetches_total{{game=\"{game}\",outcome=\"{outcome}\"}} {count}"
        );
    }
    let _ = writeln!(
        out,
        "# HELP ahd_codes_rejected_rows_total Code source rows rejected by validation since start, by game.\n# TYPE ahd_codes_rejected_rows_total counter"
    );
    for (game, count) in stats.rejected() {
        let _ = writeln!(
            out,
            "ahd_codes_rejected_rows_total{{game=\"{game}\"}} {count}"
        );
    }
    let _ = writeln!(
        out,
        "# HELP ahd_codes_announcements_total New-code summaries sent to users since start.\n# TYPE ahd_codes_announcements_total counter\nahd_codes_announcements_total {}",
        stats.announcements()
    );
    let _ = writeln!(
        out,
        "# HELP ahd_code_redemptions_total Redemption requests answered since start, by game and outcome (unknown: transport error or unclassified answer).\n# TYPE ahd_code_redemptions_total counter"
    );
    let answers = app
        .redemption
        .as_ref()
        .map(|runs| runs.stats().answers())
        .unwrap_or_else(|| crate::dashboard::codes::redeem::RedeemStats::default().answers());
    for (game, outcome, count) in answers {
        let _ = writeln!(
            out,
            "ahd_code_redemptions_total{{game=\"{game}\",outcome=\"{outcome}\"}} {count}"
        );
    }
    let rows = app
        .accounts
        .db
        .query_all_raw(Statement::from_string(DbBackend::Postgres, LEDGER))
        .await?;
    let mut states = std::collections::BTreeMap::new();
    for row in rows {
        let label: String = row.try_get("", "label")?;
        let count: i64 = row.try_get("", "n")?;
        states.insert(label, count);
    }
    let _ = writeln!(
        out,
        "# HELP ahd_code_ledger Redemption ledger rows by state (sending: a request may be in flight; unknown: never sent again automatically).\n# TYPE ahd_code_ledger gauge"
    );
    for state in crate::dashboard::codes::ledger::STATES {
        let count = states.get(state).copied().unwrap_or(0);
        let _ = writeln!(out, "ahd_code_ledger{{state=\"{state}\"}} {count}");
    }
    let counts = crate::dashboard::codes::admin::redemption_counts(&app.accounts.db).await?;
    let _ = writeln!(
        out,
        "# HELP ahd_code_auto_users Users with a game UID in automatic redemption.\n# TYPE ahd_code_auto_users gauge\nahd_code_auto_users {}",
        counts.auto_users
    );
    let _ = writeln!(
        out,
        "# HELP ahd_code_action_required_users Users whose code redemption waits for their action (CAPTCHA or Cookie).\n# TYPE ahd_code_action_required_users gauge\nahd_code_action_required_users {}",
        counts.action_required_users
    );
    Ok(())
}

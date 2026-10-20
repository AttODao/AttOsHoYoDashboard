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
const GROUPS: [(&str, &str, &str); 4] = [
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
];

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
    Ok((
        [(
            header::CONTENT_TYPE,
            "text/plain; version=0.0.4; charset=utf-8",
        )],
        out,
    ))
}

//! Home-screen widgets: what the app hands to Kotlin (`AhdWidgets.publish`) and the JNI calls.
use crate::models::GameStatus;
use serde::Serialize;
use std::collections::BTreeMap;
use uuid::Uuid;

#[cfg(feature = "mobile")]
const WIDGETS: &str = "dev.dioxus.main.AhdWidgets";

#[derive(Clone, PartialEq, Serialize)]
pub(crate) struct Publication {
    pub(crate) agent: String,
    pub(crate) games: BTreeMap<&'static str, GameEntry>,
}
#[derive(Clone, PartialEq, Serialize)]
pub(crate) struct GameEntry {
    pub(crate) account: Option<Uuid>,
    pub(crate) status: Option<GameStatus>,
}

/// Select one account per game, using a status only when it belongs to that account.
pub(crate) fn publication(
    agent: &str,
    games: &[&'static str],
    selected: impl Fn(&str) -> Option<Uuid>,
    statuses: &[(Uuid, &GameStatus)],
) -> Publication {
    Publication {
        agent: agent.to_owned(),
        games: games
            .iter()
            .map(|&game| {
                let account = selected(game);
                let status = account.and_then(|id| {
                    statuses
                        .iter()
                        .find(|(owner, _)| *owner == id)
                        .map(|(_, status)| (*status).clone())
                });
                (game, GameEntry { account, status })
            })
            .collect(),
    }
}

/// False when the Android bridge is unavailable.
#[cfg(feature = "mobile")]
pub(crate) fn publish(origin: &str, publication: &Publication) -> bool {
    let Ok(json) = serde_json::to_string(publication) else {
        return false;
    };
    super::notify::call_on(
        WIDGETS,
        "publish",
        "(Landroid/content/Context;Ljava/lang/String;Ljava/lang/String;)Z",
        Some((origin, &json)),
        true,
    )
    .as_deref()
        == Some("true")
}

#[cfg(feature = "mobile")]
pub(crate) fn sign_out() {
    super::notify::call_on(
        WIDGETS,
        "signOut",
        "(Landroid/content/Context;)Z",
        None,
        true,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn status(game: &str, uid: &str) -> GameStatus {
        serde_json::from_value(json!({"game_kind":game,"game_uid":uid,"region":"test","visible":true,"detected":true,"data":{"current_resin":100,"max_resin":200},"fetched_at":"2026-11-04T00:00:00Z","stale":false})).unwrap()
    }

    #[test]
    fn widgets_publication_selects_each_game_account_and_ignores_other_statuses() {
        let a = Uuid::from_u128(1);
        let b = Uuid::from_u128(2);
        let c = Uuid::from_u128(3);
        let selected_status = status("genshin", "123");
        let other_status = status("genshin", "456");
        let rail_status = status("starrail", "789");
        let p = publication(
            "test-agent",
            &["genshin", "starrail"],
            |g| Some(if g == "genshin" { a } else { c }),
            &[(b, &other_status), (a, &selected_status), (c, &rail_status)],
        );
        assert_eq!(p.games["genshin"].account, Some(a));
        assert!(p.games["genshin"].status.as_ref() == Some(&selected_status));
        assert!(p.games["starrail"].status.as_ref() == Some(&rail_status));
        let unloaded = publication(
            "test-agent",
            &["genshin"],
            |_| Some(c),
            &[(b, &other_status)],
        );
        assert!(unloaded.games["genshin"].status.is_none());
    }

    #[test]
    fn widgets_publication_serializes_exact_bridge_shape_and_empty_selection() {
        let id = Uuid::from_u128(1);
        let s = status("genshin", "123");
        let p = publication(
            "test-agent",
            &["genshin", "starrail"],
            |g| (g == "genshin").then_some(id),
            &[(id, &s)],
        );
        assert!(p.games["starrail"].account.is_none());
        assert!(p.games["starrail"].status.is_none());
        assert_eq!(
            serde_json::to_value(p).unwrap(),
            json!({"agent":"test-agent","games":{"genshin":{"account":id,"status":s},"starrail":{"account":null,"status":null}}})
        );
    }
}

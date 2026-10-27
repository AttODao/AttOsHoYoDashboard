//! Platform-free prediction, persistence and localization of device reminders.
use crate::{i18n::Language, models::GameStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Keep due reminders for an hour so a schedule re-push does not cancel an
/// inexact Android alarm that is still waiting for delivery.
pub(crate) const GRACE_MS: i64 = 60 * 60 * 1000;

/// Sign-out reasons after which the account no longer uses this device: logout, an expired or revoked session (401)
/// and account deletion. Any other reason (network or server errors during the session check) keeps the schedule.
const FORGETTING_SIGN_OUTS: [&str; 5] = [
    "messages.signed_out",
    "messages.logout_unconfirmed",
    "messages.please_sign_in",
    "messages.please_sign_in_again",
    "deletion.user_deletion_accepted_processing_pending",
];
pub(crate) fn forgets_schedule(is_signed_out: bool, status_key: Option<&str>) -> bool {
    is_signed_out && status_key.is_some_and(|key| FORGETTING_SIGN_OUTS.contains(&key))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Kind {
    ResourceFull,
    RealmCurrencyFull,
    ExpeditionsDone,
    TransformerReady,
}
impl Kind {
    pub(crate) fn code(self) -> &'static str {
        match self {
            Self::ResourceFull => "resource_full",
            Self::RealmCurrencyFull => "realm_currency_full",
            Self::ExpeditionsDone => "expeditions_done",
            Self::TransformerReady => "transformer_ready",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Settings {
    pub(crate) enabled: bool,
    pub(crate) resource_full: bool,
    pub(crate) realm_currency_full: bool,
    pub(crate) expeditions_done: bool,
    pub(crate) transformer_ready: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            enabled: false,
            resource_full: true,
            realm_currency_full: true,
            expeditions_done: true,
            transformer_ready: true,
        }
    }
}
impl Settings {
    pub(crate) fn from_json(text: Option<&str>) -> Self {
        text.and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    pub(crate) fn allows(&self, kind: Kind) -> bool {
        self.enabled && self.kind(kind)
    }
    pub(crate) fn kind(&self, kind: Kind) -> bool {
        match kind {
            Kind::ResourceFull => self.resource_full,
            Kind::RealmCurrencyFull => self.realm_currency_full,
            Kind::ExpeditionsDone => self.expeditions_done,
            Kind::TransformerReady => self.transformer_ready,
        }
    }
    /// The settings page has one "this device" choice per notice: the account's app inbox
    /// choice also decides the predicted reminder of the same notice, and the realm currency
    /// (no server notice) has its own device-only choice.
    pub(crate) fn follow_choices(
        &mut self,
        app_events: &std::collections::BTreeMap<String, bool>,
        realm_currency: bool,
    ) {
        for kind in [
            Kind::ResourceFull,
            Kind::ExpeditionsDone,
            Kind::TransformerReady,
        ] {
            let on = app_events.get(kind.code()).copied().unwrap_or(false);
            self.set_kind(kind, on);
        }
        self.realm_currency_full = realm_currency;
    }
    pub(crate) fn set_kind(&mut self, kind: Kind, on: bool) {
        match kind {
            Kind::ResourceFull => self.resource_full = on,
            Kind::RealmCurrencyFull => self.realm_currency_full = on,
            Kind::ExpeditionsDone => self.expeditions_done = on,
            Kind::TransformerReady => self.transformer_ready = on,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct Reminder {
    pub(crate) account: Uuid,
    pub(crate) game: String,
    pub(crate) uid: String,
    pub(crate) kind: Kind,
    pub(crate) at_ms: i64,
}
fn timestamp(value: Option<&str>) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(value?)
        .ok()
        .map(|t| t.timestamp_millis())
}
/// Every predictable event, independent of settings and clock.
pub(crate) fn reminders_for(account: Uuid, status: &GameStatus) -> Vec<Reminder> {
    let mut result = Vec::new();
    if !status.visible || !matches!(status.game_kind.as_str(), "genshin" | "starrail") {
        return result;
    }
    let Some(data) = &status.data else {
        return result;
    };
    let mut add = |kind, at_ms: Option<i64>| {
        if let Some(at_ms) = at_ms {
            result.push(Reminder {
                account,
                game: status.game_kind.clone(),
                uid: status.game_uid.clone(),
                kind,
                at_ms,
            });
        }
    };
    let below =
        |current: Option<i64>, max: Option<i64>| current.zip(max).is_some_and(|(c, m)| c < m);
    if status.game_kind == "genshin" {
        if below(data.current_resin, data.max_resin) {
            add(Kind::ResourceFull, timestamp(data.resin_full_at.as_deref()));
        }
        if below(data.current_home_coin, data.max_home_coin) {
            add(
                Kind::RealmCurrencyFull,
                timestamp(data.home_coin_full_at.as_deref()),
            );
        }
        if data.transformer_obtained == Some(true) && data.transformer_ready != Some(true) {
            add(
                Kind::TransformerReady,
                timestamp(data.transformer_ready_at.as_deref()),
            );
        }
    } else if below(data.current_stamina, data.max_stamina) {
        add(
            Kind::ResourceFull,
            timestamp(data.stamina_full_at.as_deref()),
        );
    }
    let seconds = data
        .expeditions
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|e| e.status.as_deref() != Some("Finished"))
        .filter_map(|e| {
            e.remained_time
                .as_ref()
                .or(e.remaining_time.as_ref())?
                .seconds()
        })
        .filter(|s| *s > 0)
        .max();
    let at = timestamp(status.fetched_at.as_deref())
        .zip(seconds)
        .and_then(|(at, seconds)| at.checked_add(seconds.checked_mul(1000)?));
    add(Kind::ExpeditionsDone, at);
    result
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct Schedule {
    pub(crate) owner: Option<Uuid>,
    pub(crate) reminders: Vec<Reminder>,
}
impl Schedule {
    pub(crate) fn from_json(text: Option<&str>) -> Self {
        text.and_then(|s| serde_json::from_str(s).ok())
            .unwrap_or_default()
    }
    pub(crate) fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    pub(crate) fn update_account(&mut self, owner: Uuid, account: Uuid, status: &GameStatus) {
        if self.owner != Some(owner) {
            self.clear();
            self.owner = Some(owner);
        }
        self.reminders.retain(|r| r.account != account);
        self.reminders.extend(reminders_for(account, status));
    }
    pub(crate) fn retain_accounts(&mut self, accounts: &[Uuid]) {
        self.reminders.retain(|r| accounts.contains(&r.account));
    }
    pub(crate) fn drop_past(&mut self, now_ms: i64) {
        self.reminders.retain(|r| r.at_ms > now_ms - GRACE_MS);
    }
    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }
    pub(crate) fn upcoming(&self, settings: &Settings, now_ms: i64) -> Vec<&Reminder> {
        let mut reminders: Vec<_> = self
            .reminders
            .iter()
            .filter(|r| r.at_ms > now_ms - GRACE_MS && settings.allows(r.kind))
            .collect();
        reminders.sort_by_key(|r| (r.at_ms, r.kind));
        reminders
    }
}
pub(crate) fn notification_id(reminder: &Reminder) -> i32 {
    let mut hash = 2_166_136_261_u32;
    for byte in format!("{}:{}", reminder.account, reminder.kind.code()).bytes() {
        hash = (hash ^ u32::from(byte)).wrapping_mul(16_777_619);
    }
    (2 + hash % (i32::MAX as u32 - 1)) as i32
}
pub(crate) const TEST_NOTIFICATION_ID: i32 = 1;
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub(crate) struct Outgoing {
    pub(crate) id: i32,
    pub(crate) at: i64,
    pub(crate) title: String,
    pub(crate) body: String,
    /// "<game account>:<kind>", the server's key for the same condition: Android shows only
    /// one of a predicted and a server notice with the same tag (MainActivity.kt).
    pub(crate) tag: String,
}
pub(crate) fn render(reminders: &[&Reminder], language: Language) -> Vec<Outgoing> {
    reminders
        .iter()
        .map(|r| {
            let title = match r.kind {
                Kind::ResourceFull | Kind::RealmCurrencyFull => {
                    let key = if r.kind == Kind::RealmCurrencyFull {
                        "dashboard.realm_currency"
                    } else if r.game == "starrail" {
                        "dashboard.trailblaze_power"
                    } else {
                        "dashboard.resin"
                    };
                    language.format(
                        "notifications.device_full_title",
                        &[("resource", language.text(key))],
                    )
                }
                Kind::ExpeditionsDone => language
                    .text("notifications.device_expeditions_title")
                    .to_owned(),
                Kind::TransformerReady => language
                    .text("notifications.device_transformer_title")
                    .to_owned(),
            };
            let game = language.text(if r.game == "starrail" {
                "dashboard.honkai_star_rail"
            } else {
                "dashboard.genshin_impact"
            });
            Outgoing {
                id: notification_id(r),
                at: r.at_ms,
                tag: format!("{}:{}", r.account, r.kind.code()),
                title,
                body: language.format(
                    "notifications.device_body",
                    &[("game", game), ("uid", &r.uid)],
                ),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};
    const DATE: &str = "2026-01-01T00:00:00Z";
    #[test]
    fn only_confirmed_sign_outs_forget_the_schedule() {
        let preserved_keys = [
            None,
            Some("messages.the_dashboard_could_not_load_your_session"),
            Some("messages.the_server_could_not_be_reached"),
        ];
        let sign_out_keys = [
            "messages.signed_out",
            "messages.logout_unconfirmed",
            "messages.please_sign_in",
            "messages.please_sign_in_again",
            "deletion.user_deletion_accepted_processing_pending",
        ];
        // SignedIn, Loading and Unchecked all pass false to the pure helper.
        for key in preserved_keys
            .into_iter()
            .chain(sign_out_keys.into_iter().map(Some))
        {
            assert!(!forgets_schedule(false, key));
        }
        for key in preserved_keys {
            assert!(!forgets_schedule(true, key));
        }
        for key in sign_out_keys {
            assert!(forgets_schedule(true, Some(key)));
        }
    }
    fn status(game: &str, data: Value) -> GameStatus {
        serde_json::from_value(json!({"game_kind":game,"game_uid":"123","region":"test","visible":true,"detected":true,"data":data,"fetched_at":DATE,"stale":false})).unwrap()
    }
    fn predictions(game: &str, data: Value) -> Vec<Reminder> {
        reminders_for(Uuid::nil(), &status(game, data))
    }
    #[test]
    fn resources_require_below_cap_and_valid_timestamp() {
        for (game, current, max, at) in [
            ("genshin", "current_resin", "max_resin", "resin_full_at"),
            (
                "starrail",
                "current_stamina",
                "max_stamina",
                "stamina_full_at",
            ),
            (
                "genshin",
                "current_home_coin",
                "max_home_coin",
                "home_coin_full_at",
            ),
        ] {
            assert_eq!(predictions(game, json!({current:1,max:2,at:DATE})).len(), 1);
            for c in [2, 3] {
                assert!(predictions(game, json!({current:c,max:2,at:DATE})).is_empty());
            }
            assert!(predictions(game, json!({current:1,max:2,at:"bad"})).is_empty());
            assert!(predictions(game, json!({max:2,at:DATE})).is_empty());
        }
    }
    #[test]
    fn transformer_requires_obtained_and_not_ready() {
        for obtained in [None, Some(false), Some(true)] {
            for ready in [None, Some(false), Some(true)] {
                let reminders = predictions(
                    "genshin",
                    json!({"transformer_obtained":obtained,"transformer_ready":ready,"transformer_ready_at":DATE}),
                );
                assert_eq!(
                    reminders.len(),
                    usize::from(obtained == Some(true) && ready != Some(true))
                );
            }
        }
        assert!(
            predictions(
                "starrail",
                json!({"transformer_obtained":true,"transformer_ready_at":DATE})
            )
            .is_empty()
        );
    }
    #[test]
    fn expeditions_take_max_positive_unfinished_duration() {
        for game in ["genshin", "starrail"] {
            let mut s = status(
                game,
                json!({"expeditions":[{"status":"Finished","remained_time":999},{"remained_time":"120"},{"remaining_time":240},{"remained_time":-1},{"remained_time":"bad"}]}),
            );
            let r = reminders_for(Uuid::nil(), &s);
            assert_eq!(r[0].at_ms, timestamp(Some(DATE)).unwrap() + 240_000);
            s.fetched_at = None;
            assert!(reminders_for(Uuid::nil(), &s).is_empty());
            assert!(
                predictions(
                    game,
                    json!({"expeditions":[{"status":"Finished","remaining_time":50}]})
                )
                .is_empty()
            );
        }
    }
    #[test]
    fn invisible_missing_and_unknown_statuses_are_ignored() {
        let mut s = status(
            "genshin",
            json!({"current_resin":1,"max_resin":2,"resin_full_at":DATE}),
        );
        s.visible = false;
        assert!(reminders_for(Uuid::nil(), &s).is_empty());
        s.visible = true;
        s.data = None;
        assert!(reminders_for(Uuid::nil(), &s).is_empty());
        assert!(
            predictions(
                "other",
                json!({"current_resin":1,"max_resin":2,"resin_full_at":DATE})
            )
            .is_empty()
        );
    }
    #[test]
    fn schedule_replacement_ownership_retention_and_time() {
        let owner = Uuid::from_u128(1);
        let a = Uuid::from_u128(2);
        let b = Uuid::from_u128(3);
        let s = status(
            "genshin",
            json!({"current_resin":1,"max_resin":2,"resin_full_at":DATE}),
        );
        let mut schedule = Schedule::default();
        schedule.update_account(owner, a, &s);
        schedule.update_account(owner, b, &s);
        schedule.update_account(owner, a, &s);
        assert_eq!(schedule.reminders.len(), 2);
        assert_eq!(Schedule::from_json(Some(&schedule.to_json())), schedule);
        schedule.retain_accounts(&[b]);
        assert_eq!(schedule.reminders[0].account, b);
        schedule.update_account(b, a, &s);
        assert_eq!(schedule.reminders.len(), 1);
        assert_eq!(schedule.owner, Some(b));
        schedule.drop_past(timestamp(Some(DATE)).unwrap() + GRACE_MS);
        assert!(schedule.reminders.is_empty());
        schedule.clear();
        assert_eq!(schedule, Schedule::default());
        assert_eq!(Schedule::from_json(Some("bad")), Schedule::default());
    }
    #[test]
    fn grace_window_excludes_boundary_and_keeps_pending_due_reminders() {
        let now = GRACE_MS + 100;
        let cutoff = now - GRACE_MS;
        let times = [cutoff - 1, cutoff, cutoff + 1, now, now + 1];
        let mut schedule = Schedule {
            owner: None,
            reminders: times
                .into_iter()
                .rev()
                .map(|at_ms| Reminder {
                    account: Uuid::nil(),
                    game: "genshin".into(),
                    uid: "123".into(),
                    kind: Kind::ResourceFull,
                    at_ms,
                })
                .collect(),
        };
        let settings = Settings {
            enabled: true,
            ..Settings::default()
        };
        let expected = vec![cutoff + 1, now, now + 1];
        assert_eq!(
            schedule
                .upcoming(&settings, now)
                .iter()
                .map(|r| r.at_ms)
                .collect::<Vec<_>>(),
            expected
        );
        schedule.drop_past(now);
        let mut retained: Vec<_> = schedule.reminders.iter().map(|r| r.at_ms).collect();
        retained.sort();
        assert_eq!(retained, expected);
    }
    #[test]
    fn device_choices_steer_predicted_kinds() {
        let mut settings = Settings {
            enabled: true,
            ..Settings::default()
        };
        let chosen = std::collections::BTreeMap::from([
            ("resource_full".to_owned(), false),
            ("expeditions_done".to_owned(), true),
            ("signed_in".to_owned(), true),
        ]);
        settings.follow_choices(&chosen, false);
        assert!(settings.enabled);
        assert!(!settings.kind(Kind::ResourceFull));
        assert!(!settings.kind(Kind::RealmCurrencyFull));
        assert!(settings.kind(Kind::ExpeditionsDone));
        // A notice missing from the choices is off, like on the server.
        assert!(!settings.kind(Kind::TransformerReady));
        settings.follow_choices(&chosen, true);
        assert!(settings.kind(Kind::RealmCurrencyFull));
    }
    #[test]
    fn settings_defaults_switches_sorting_and_ids() {
        let mut settings = Settings::default();
        for text in [None, Some("bad"), Some("{}")] {
            assert_eq!(Settings::from_json(text), settings);
        }
        assert!(!settings.enabled);
        let kinds = [
            Kind::ResourceFull,
            Kind::RealmCurrencyFull,
            Kind::ExpeditionsDone,
            Kind::TransformerReady,
        ];
        let mut schedule = Schedule::default();
        for (i, kind) in kinds.into_iter().rev().enumerate() {
            schedule.reminders.push(Reminder {
                account: Uuid::nil(),
                game: "genshin".into(),
                uid: "123".into(),
                kind,
                at_ms: if i == 0 { 20 } else { 10 },
            });
        }
        assert!(schedule.upcoming(&settings, 0).is_empty());
        settings.enabled = true;
        let upcoming = schedule.upcoming(&settings, 0);
        assert_eq!(upcoming.iter().map(|r| r.kind).collect::<Vec<_>>(), kinds);
        assert!(schedule.upcoming(&settings, 20 + GRACE_MS).is_empty());
        let ids: std::collections::BTreeSet<_> =
            schedule.reminders.iter().map(notification_id).collect();
        assert_eq!(ids.len(), 4);
        assert!(ids.iter().all(|id| *id >= 2));
        assert_eq!(TEST_NOTIFICATION_ID, 1);
        assert_eq!(notification_id(upcoming[0]), 2_116_401_159);
        for kind in kinds {
            assert!(settings.kind(kind));
            settings.set_kind(kind, false);
            assert!(!settings.allows(kind));
            assert!(
                !schedule
                    .upcoming(&settings, 0)
                    .iter()
                    .any(|r| r.kind == kind)
            );
            settings.set_kind(kind, true);
        }
        assert_eq!(Settings::from_json(Some(&settings.to_json())), settings);
    }
    #[test]
    fn english_render_uses_game_resource_names() {
        for (game, kind, resource) in [
            ("genshin", Kind::ResourceFull, "Resin"),
            ("starrail", Kind::ResourceFull, "Trailblaze Power"),
            ("genshin", Kind::RealmCurrencyFull, "Realm currency"),
        ] {
            let r = Reminder {
                account: Uuid::nil(),
                game: game.into(),
                uid: "123".into(),
                kind,
                at_ms: 1,
            };
            let out = render(&[&r], Language::English);
            assert_eq!(out[0].title, format!("{resource} is full"));
            assert_eq!(out[0].tag, format!("{}:{}", Uuid::nil(), kind.code()));
            assert!(out[0].body.contains("123"));
        }
    }
}

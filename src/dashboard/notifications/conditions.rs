//! Pure rules turning one game's summary into notices, or clearing a past condition.
use super::{Notice, messages::word};
use crate::dashboard::rewards::schedule::ScheduleZone;
use chrono::{DateTime, Datelike, Duration, NaiveTime, Utc, Weekday};
use serde_json::Value;
use uuid::Uuid;

/// The user's thresholds and reminder clock times (in the server's schedule zone).
#[derive(Clone, Copy, Debug)]
pub(super) struct Rules {
    pub(super) warn_hours: i64,
    pub(super) daily: NaiveTime,
    pub(super) weekly: NaiveTime,
    pub(super) zone: ScheduleZone,
}

/// A condition to report (once) or one that no longer holds and may report again later.
#[derive(Clone, Debug, PartialEq)]
pub(super) enum Outcome {
    Fire(Notice),
    Clear(String),
}

pub(super) struct Game<'a> {
    pub(super) account: Uuid,
    pub(super) kind: &'a str,
    pub(super) uid: &'a str,
    pub(super) summary: &'a Value,
}

/// The game's primary resource as one fetched snapshot reported it.
struct Resource {
    current: i64,
    max: i64,
    full_at: Option<DateTime<Utc>>,
    word: &'static str,
}

fn resource(game: &Game<'_>) -> Option<Resource> {
    let (current_key, max_key, full_key, word) = match game.kind {
        "genshin" => ("current_resin", "max_resin", "resin_full_at", "resin"),
        _ => (
            "current_stamina",
            "max_stamina",
            "stamina_full_at",
            "stamina",
        ),
    };
    let int = |key: &str| game.summary.get(key).and_then(Value::as_i64);
    let full_at = game
        .summary
        .get(full_key)
        .and_then(Value::as_str)
        .and_then(|at| DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.with_timezone(&Utc));
    Some(Resource {
        current: int(current_key)?,
        max: int(max_key)?,
        full_at,
        word,
    })
}

/// The first moment after `observed_at` at which the snapshot's own projection says a
/// resource notice would start to hold: the near-cap window opening, then the cap.
/// A projection is never reported by itself; the worker fetches a confirming status
/// once this moment has passed.
pub(super) fn next_check(
    game: &Game<'_>,
    warn_hours: i64,
    observed_at: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    let resource = resource(game)?;
    if resource.max <= 0 || resource.current >= resource.max {
        return None;
    }
    let full_at = resource.full_at?;
    [full_at - Duration::hours(warn_hours), full_at]
        .into_iter()
        .find(|at| *at > observed_at)
}

/// Evaluates one fetched snapshot. The game-status conditions (resources, expeditions,
/// transformer) are what HoYoLAB reported at `observed_at`, the snapshot's fetch time:
/// a projected cap that passed after the fetch is not a cap. The daily and weekly
/// reminders follow the clock (`now`).
pub(super) fn evaluate(
    game: &Game<'_>,
    rules: Rules,
    observed_at: DateTime<Utc>,
    now: DateTime<Utc>,
    language: &str,
) -> Vec<Outcome> {
    let mut out = Vec::new();
    let name = word(game.kind, language);
    let int = |key: &str| game.summary.get(key).and_then(Value::as_i64);
    let local = now.with_timezone(&rules.zone.offset_at(now));
    let base = |kind: &'static str, key: String| {
        Notice::new(kind, key)
            .with("game", name)
            .with("uid", game.uid.to_owned())
    };

    // Primary resource: near the cap within the warning window, then full.
    if let Some(Resource {
        current,
        max,
        full_at,
        word: resource,
    }) = resource(game)
    {
        let full = max > 0 && current >= max;
        let near = !full
            && full_at.is_some_and(|at| at - observed_at <= Duration::hours(rules.warn_hours));
        let near_key = format!("{}:resource_near_cap", game.account);
        let full_key = format!("{}:resource_full", game.account);
        let shown = if full { max } else { current };
        let with_values = |notice: Notice| {
            notice
                .with("resource", word(resource, language))
                .with("current", shown.to_string())
                .with("max", max.to_string())
        };
        if full {
            out.push(Outcome::Fire(with_values(base("resource_full", full_key))));
        } else {
            out.push(Outcome::Clear(full_key));
        }
        if near {
            let at = full_at
                .map(|at| {
                    at.with_timezone(&rules.zone.offset_at(at))
                        .format("%m/%d %H:%M")
                        .to_string()
                })
                .unwrap_or_default();
            out.push(Outcome::Fire(
                with_values(base("resource_near_cap", near_key)).with("at", at),
            ));
        } else if !full {
            // Spending below the window re-arms the warning; full keeps it quiet.
            out.push(Outcome::Clear(near_key));
        }
    }

    // Expeditions: report once when every running expedition has finished.
    if let Some(rows) = game.summary.get("expeditions").and_then(Value::as_array) {
        let key = format!("{}:expeditions_done", game.account);
        let done = !rows.is_empty()
            && rows
                .iter()
                .all(|row| row.get("status").and_then(Value::as_str) == Some("Finished"));
        if done {
            out.push(Outcome::Fire(
                base("expeditions_done", key).with("count", rows.len().to_string()),
            ));
        } else {
            out.push(Outcome::Clear(key));
        }
    }

    if let Some(ready) = game
        .summary
        .get("transformer_ready")
        .and_then(Value::as_bool)
    {
        let key = format!("{}:transformer_ready", game.account);
        if ready {
            out.push(Outcome::Fire(base("transformer_ready", key)));
        } else {
            out.push(Outcome::Clear(key));
        }
    }

    // Daily reminder, once per local day after the chosen time.
    if local.time() >= rules.daily {
        let unfinished = match game.kind {
            "genshin" => int("finished_task_num")
                .zip(int("total_task_num"))
                .filter(|(done, total)| done < total),
            _ => int("current_train_score")
                .zip(int("max_train_score"))
                .filter(|(done, total)| done < total),
        };
        if let Some((done, total)) = unfinished {
            let key = format!("{}:daily:{}", game.account, local.date_naive());
            out.push(Outcome::Fire(
                base("daily_unfinished", key).with("progress", format!("{done}/{total}")),
            ));
        }
    }

    // Weekly reminder, Sundays after the chosen time, once per ISO week.
    if local.weekday() == Weekday::Sun && local.time() >= rules.weekly {
        let mut items = Vec::new();
        if int("remain_resin_discount_num").is_some_and(|left| left > 0) {
            items.push(word("weekly_boss", language));
        }
        if int("weekly_cocoon_cnt").is_some_and(|left| left > 0) {
            items.push(word("echo_of_war", language));
        }
        if int("current_rogue_score")
            .zip(int("max_rogue_score"))
            .is_some_and(|(score, max)| score < max)
        {
            items.push(word("simulated_universe", language));
        }
        if !items.is_empty() {
            let week = local.iso_week();
            let key = format!("{}:weekly:{}-{}", game.account, week.year(), week.week());
            let separator = if language == "en" { ", " } else { "、" };
            out.push(Outcome::Fire(
                base("weekly_unfinished", key).with("items", items.join(separator)),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;
    use serde_json::json;

    fn rules() -> Rules {
        Rules {
            warn_hours: 2,
            daily: NaiveTime::from_hms_opt(21, 0, 0).unwrap(),
            weekly: NaiveTime::from_hms_opt(15, 0, 0).unwrap(),
            zone: ScheduleZone::Fixed(FixedOffset::east_opt(9 * 3600).unwrap()),
        }
    }

    fn at(value: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(value)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn fired(outcomes: &[Outcome]) -> Vec<&'static str> {
        outcomes
            .iter()
            .filter_map(|outcome| match outcome {
                Outcome::Fire(notice) => Some(notice.kind),
                Outcome::Clear(_) => None,
            })
            .collect()
    }

    #[test]
    fn resources_warn_before_the_cap_then_report_full_and_re_arm_when_spent() {
        let account = Uuid::nil();
        let now = at("2026-10-21T03:00:00Z"); // Wednesday 12:00 JST
        let near = json!({"current_resin": 190, "max_resin": 200, "resin_full_at": "2026-10-21T04:20:00Z"});
        let game = Game {
            account,
            kind: "genshin",
            uid: "800000001",
            summary: &near,
        };
        let outcomes = evaluate(&game, rules(), now, now, "ja");
        assert_eq!(fired(&outcomes), ["resource_near_cap"]);
        let Outcome::Fire(notice) = outcomes
            .iter()
            .find(|o| matches!(o, Outcome::Fire(_)))
            .unwrap()
        else {
            unreachable!()
        };
        assert!(notice.params.contains(&("at", "10/21 13:20".to_owned())));
        assert!(outcomes.contains(&Outcome::Clear(format!("{account}:resource_full"))));

        let full = json!({"current_stamina": 300, "max_stamina": 300});
        let game = Game {
            account,
            kind: "starrail",
            uid: "700000001",
            summary: &full,
        };
        let outcomes = evaluate(&game, rules(), now, now, "en");
        assert_eq!(fired(&outcomes), ["resource_full"]);
        // Full keeps the near-cap key armed-off: no clear for it.
        assert!(!outcomes.contains(&Outcome::Clear(format!("{account}:resource_near_cap"))));

        let spent =
            json!({"current_resin": 20, "max_resin": 200, "resin_full_at": "2026-10-22T00:00:00Z"});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &spent,
        };
        let outcomes = evaluate(&game, rules(), now, now, "ja");
        assert!(fired(&outcomes).is_empty());
        assert!(outcomes.contains(&Outcome::Clear(format!("{account}:resource_full"))));
        assert!(outcomes.contains(&Outcome::Clear(format!("{account}:resource_near_cap"))));
    }

    #[test]
    fn a_projection_alone_never_reports_and_names_the_moment_to_confirm() {
        let account = Uuid::nil();
        let fetched = at("2026-10-21T00:00:00Z");
        let now = at("2026-10-21T06:00:00Z");
        // Fetched at 09:00 JST with the cap projected for 13:20 JST (2h warning).
        let old = json!({"current_resin": 160, "max_resin": 200, "resin_full_at": "2026-10-21T04:20:00Z"});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &old,
        };
        // Long after the projected cap, the snapshot still reports what was fetched.
        let outcomes = evaluate(&game, rules(), fetched, now, "ja");
        assert!(fired(&outcomes).is_empty());
        assert!(outcomes.contains(&Outcome::Clear(format!("{account}:resource_full"))));
        assert!(outcomes.contains(&Outcome::Clear(format!("{account}:resource_near_cap"))));
        // The window opens at 11:20 JST, then the cap at 13:20 JST.
        assert_eq!(
            next_check(&game, 2, fetched),
            Some(at("2026-10-21T02:20:00Z"))
        );
        assert_eq!(
            next_check(&game, 2, at("2026-10-21T02:20:00Z")),
            Some(at("2026-10-21T04:20:00Z"))
        );
        assert_eq!(next_check(&game, 2, at("2026-10-21T04:20:00Z")), None);
        // Inside the window at the fetch: the warning fires from that fetch.
        assert_eq!(
            fired(&evaluate(
                &game,
                rules(),
                at("2026-10-21T03:00:00Z"),
                now,
                "ja"
            )),
            ["resource_near_cap"]
        );
        // Full (or without a projection) has nothing left to confirm.
        let full = json!({"current_stamina": 240, "max_stamina": 240, "stamina_full_at": "2026-10-21T04:20:00Z"});
        let game = Game {
            account,
            kind: "starrail",
            uid: "1",
            summary: &full,
        };
        assert_eq!(next_check(&game, 2, fetched), None);
        let unknown = json!({"current_resin": 100, "max_resin": 200});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &unknown,
        };
        assert_eq!(next_check(&game, 2, fetched), None);
    }

    #[test]
    fn expeditions_and_transformer_report_when_ready() {
        let account = Uuid::nil();
        let now = at("2026-10-21T03:00:00Z");
        let ready = json!({"expeditions": [{"status": "Finished"}, {"status": "Finished"}], "transformer_ready": true});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &ready,
        };
        assert_eq!(
            fired(&evaluate(&game, rules(), now, now, "ja")),
            ["expeditions_done", "transformer_ready"]
        );
        let running = json!({"expeditions": [{"status": "Finished"}, {"status": "Ongoing"}], "transformer_ready": false});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &running,
        };
        let outcomes = evaluate(&game, rules(), now, now, "ja");
        assert!(fired(&outcomes).is_empty());
        assert_eq!(outcomes.len(), 2);
        let none = json!({"expeditions": []});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &none,
        };
        assert!(fired(&evaluate(&game, rules(), now, now, "ja")).is_empty());
    }

    #[test]
    fn daily_and_weekly_reminders_wait_for_their_times_and_are_keyed_per_period() {
        let account = Uuid::nil();
        let summary =
            json!({"finished_task_num": 2, "total_task_num": 4, "remain_resin_discount_num": 1});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &summary,
        };
        // Sunday 2026-10-25 14:59 JST: neither time has come.
        assert!(
            fired(&evaluate(
                &game,
                rules(),
                at("2026-10-25T05:59:00Z"),
                at("2026-10-25T05:59:00Z"),
                "ja"
            ))
            .is_empty()
        );
        // Sunday 15:00 JST: weekly only.
        let outcomes = evaluate(
            &game,
            rules(),
            at("2026-10-25T06:00:00Z"),
            at("2026-10-25T06:00:00Z"),
            "ja",
        );
        assert_eq!(fired(&outcomes), ["weekly_unfinished"]);
        let Outcome::Fire(weekly) = &outcomes[0] else {
            unreachable!()
        };
        assert_eq!(weekly.key, format!("{account}:weekly:2026-43"));
        assert!(weekly.params.contains(&("items", "週ボス".to_owned())));
        // Sunday 21:00 JST: both, the daily keyed by local date.
        let outcomes = evaluate(
            &game,
            rules(),
            at("2026-10-25T12:00:00Z"),
            at("2026-10-25T12:00:00Z"),
            "ja",
        );
        assert_eq!(fired(&outcomes), ["daily_unfinished", "weekly_unfinished"]);
        let Outcome::Fire(daily) = &outcomes[0] else {
            unreachable!()
        };
        assert_eq!(daily.key, format!("{account}:daily:2026-10-25"));
        // Monday after the weekly time: no weekly reminder.
        let done =
            json!({"finished_task_num": 4, "total_task_num": 4, "remain_resin_discount_num": 1});
        let game = Game {
            account,
            kind: "genshin",
            uid: "1",
            summary: &done,
        };
        assert!(
            fired(&evaluate(
                &game,
                rules(),
                at("2026-10-26T12:00:00Z"),
                at("2026-10-26T12:00:00Z"),
                "ja"
            ))
            .is_empty()
        );
        // Star Rail weekly items.
        let hsr = json!({"current_train_score": 500, "max_train_score": 500, "weekly_cocoon_cnt": 0, "current_rogue_score": 100, "max_rogue_score": 14000});
        let game = Game {
            account,
            kind: "starrail",
            uid: "1",
            summary: &hsr,
        };
        let outcomes = evaluate(
            &game,
            rules(),
            at("2026-10-25T12:00:00Z"),
            at("2026-10-25T12:00:00Z"),
            "en",
        );
        assert_eq!(fired(&outcomes), ["weekly_unfinished"]);
        let Outcome::Fire(weekly) = &outcomes[0] else {
            unreachable!()
        };
        assert!(
            weekly
                .params
                .contains(&("items", "Simulated Universe".to_owned()))
        );
    }
}

//! Redemption-code notices: the notify-only summary of new codes (one per user and fetch
//! run), the summary of codes redeemed for the user (one per user and redemption run) and
//! the notice that redemption waits for the user's action.
use super::{Notice, channel::Notifications, messages::word};
use crate::dashboard::error::Result;
use uuid::Uuid;

/// Codes listed in one notice; the rest are counted ("and N more").
const LISTED: usize = 10;
/// Reward text shown per code in a notice.
const REWARD_CHARS: usize = 80;

/// One code a summary announces, with its official redemption page.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::dashboard) struct NewCode {
    pub(in crate::dashboard) game_kind: String,
    pub(in crate::dashboard) code: String,
    pub(in crate::dashboard) rewards: String,
    pub(in crate::dashboard) redeem_url: String,
}

/// One code redeemed for one of the user's UIDs.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::dashboard) struct RedeemedCode {
    pub(in crate::dashboard) game_kind: String,
    pub(in crate::dashboard) game_uid: String,
    pub(in crate::dashboard) code: String,
    pub(in crate::dashboard) rewards: String,
}

impl Notifications {
    /// Sends the user one `code_redeemed` summary under `key`. True when a route took it.
    pub(in crate::dashboard) async fn announce_redeemed(
        &self,
        user: Uuid,
        key: &str,
        codes: &[RedeemedCode],
    ) -> Result<bool> {
        if codes.is_empty() {
            return Ok(false);
        }
        let (channel, _) = self.personal_channel(user).await?;
        let notice = redeemed_summary(key, codes, &channel.language);
        self.deliver(Some(user), &channel, &notice).await
    }

    /// Tells the user that redemption stopped until they act: `action` is
    /// `cookie_invalid` or `captcha_required`.
    pub(in crate::dashboard) async fn redemption_action(
        &self,
        user: Uuid,
        key: &str,
        action: &str,
    ) -> Result<bool> {
        let (channel, _) = self.personal_channel(user).await?;
        let notice = action_notice(key, action, &channel.language);
        self.deliver(Some(user), &channel, &notice).await
    }

    /// Sends the user one `code_available` summary under `key` through the routes that
    /// chose that kind. True when a route took it (see `deliver`).
    pub(in crate::dashboard) async fn announce_codes(
        &self,
        user: Uuid,
        key: &str,
        codes: &[NewCode],
    ) -> Result<bool> {
        if codes.is_empty() {
            return Ok(false);
        }
        let (channel, _) = self.personal_channel(user).await?;
        let notice = summary(key, codes, &channel.language);
        self.deliver(Some(user), &channel, &notice).await
    }
}

/// Reward text as shown in a notice line: `*` ("times") as `×`, cut to `REWARD_CHARS`.
fn reward_text(raw: &str) -> String {
    let mut rewards: String = raw.replace('*', "×");
    if rewards.chars().count() > REWARD_CHARS {
        rewards = rewards.chars().take(REWARD_CHARS - 1).collect::<String>() + "…";
    }
    rewards
}

/// The redeemed summary: one line per code (game, UID, code, rewards), at most `LISTED`.
pub(super) fn redeemed_summary(key: &str, codes: &[RedeemedCode], language: &str) -> Notice {
    let mut lines: Vec<String> = codes
        .iter()
        .take(LISTED)
        .map(|code| {
            let rewards = reward_text(&code.rewards);
            let head = format!(
                "{} UID {}: {}",
                word(&code.game_kind, language),
                code.game_uid,
                code.code
            );
            if rewards.is_empty() {
                head
            } else {
                format!("{head} ({rewards})")
            }
        })
        .collect();
    if codes.len() > LISTED {
        lines.push(
            word("codes_more", language).replace("{count}", &(codes.len() - LISTED).to_string()),
        );
    }
    Notice::new("code_redeemed", key)
        .with("count", codes.len().to_string())
        .with("codes", lines.join("\n"))
}

/// The action notice: why redemption stopped and what resumes it.
pub(super) fn action_notice(key: &str, action: &str, language: &str) -> Notice {
    let reason = match action {
        "captcha_required" => "code_action_captcha_required",
        _ => "code_action_cookie_invalid",
    };
    Notice::new("code_action_required", key).with("reason", word(reason, language))
}

/// The notice text: one line per code (game, code, rewards) and its redemption link.
/// `*` in source reward text means "times" and is shown as `×` (Discord reads `*` as markup).
pub(super) fn summary(key: &str, codes: &[NewCode], language: &str) -> Notice {
    let mut lines: Vec<String> = codes
        .iter()
        .take(LISTED)
        .map(|code| {
            let rewards = reward_text(&code.rewards);
            let game = word(&code.game_kind, language);
            let head = if rewards.is_empty() {
                format!("{game}: {}", code.code)
            } else {
                format!("{game}: {} ({rewards})", code.code)
            };
            format!("{head}\n{}", code.redeem_url)
        })
        .collect();
    if codes.len() > LISTED {
        lines.push(
            word("codes_more", language).replace("{count}", &(codes.len() - LISTED).to_string()),
        );
    }
    Notice::new("code_available", key)
        .with("count", codes.len().to_string())
        .with("codes", lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dashboard::notifications::messages::render;

    fn code(game: &str, code: &str, rewards: &str) -> NewCode {
        NewCode {
            game_kind: game.into(),
            code: code.into(),
            rewards: rewards.into(),
            redeem_url: format!("https://example.test/gift?code={code}"),
        }
    }

    #[test]
    fn one_summary_lists_each_code_with_its_link() {
        let notice = summary(
            "codes:1",
            &[
                code("genshin", "GENSHINGIFT", "Primogem*60;Mora*10,000"),
                code("starrail", "STARRAILGIFT", ""),
            ],
            "en",
        );
        assert_eq!(notice.kind, "code_available");
        assert_eq!(notice.key, "codes:1");
        let (title, body) = render(&notice, "en");
        assert_eq!(title, "2 new redemption code(s)");
        assert_eq!(
            body,
            "Genshin Impact: GENSHINGIFT (Primogem×60;Mora×10,000)\nhttps://example.test/gift?code=GENSHINGIFT\nHonkai: Star Rail: STARRAILGIFT\nhttps://example.test/gift?code=STARRAILGIFT"
        );
        let (title, _) = render(
            &summary("codes:1", &[code("genshin", "ABCD", "")], "ja"),
            "ja",
        );
        assert_eq!(title, "新しい交換コードが1件あります");
    }

    #[test]
    fn redeemed_summaries_name_the_uid_and_action_notices_say_what_resumes() {
        let redeemed = |code: &str, rewards: &str| RedeemedCode {
            game_kind: "starrail".into(),
            game_uid: "800000002".into(),
            code: code.into(),
            rewards: rewards.into(),
        };
        let notice = redeemed_summary(
            "code_redeemed:1",
            &[
                redeemed("RAILGIFT", "Stellar Jade*60"),
                redeemed("PLAINGIFT", ""),
            ],
            "en",
        );
        assert_eq!(notice.kind, "code_redeemed");
        let (title, body) = render(&notice, "en");
        assert_eq!(title, "Redeemed 2 code(s)");
        assert_eq!(
            body,
            "Honkai: Star Rail UID 800000002: RAILGIFT (Stellar Jade×60)\nHonkai: Star Rail UID 800000002: PLAINGIFT"
        );
        let (title, body) = render(
            &redeemed_summary("k", &[redeemed("RAILGIFT", "")], "ja"),
            "ja",
        );
        assert_eq!(title, "交換コードを1件引き換えました");
        assert_eq!(body, "崩壊：スターレイル UID 800000002: RAILGIFT");
        let many: Vec<_> = (0..12)
            .map(|n| redeemed(&format!("GIFT{n:04}"), ""))
            .collect();
        assert!(
            render(&redeemed_summary("k", &many, "ja"), "ja")
                .1
                .ends_with("ほか2件")
        );

        for (action, ja, en) in [
            (
                "cookie_invalid",
                "cookie_token_v2",
                "Register the Cookie again",
            ),
            ("captcha_required", "CAPTCHA", "Redeem now"),
        ] {
            let notice = action_notice("code_action:1", action, "ja");
            assert_eq!(notice.kind, "code_action_required");
            let (title, body) = render(&notice, "ja");
            assert_eq!(title, "交換コードの自動引き換えを止めました");
            assert!(body.contains(ja), "{body}");
            let (title, body) = render(&action_notice("k", action, "en"), "en");
            assert_eq!(title, "Code redemption paused");
            assert!(body.contains(en), "{body}");
        }
    }

    #[test]
    fn long_lists_and_long_rewards_are_cut() {
        let codes: Vec<_> = (0..13)
            .map(|n| code("genshin", &format!("CODE{n:04}"), &"a".repeat(200)))
            .collect();
        let notice = summary("codes:2", &codes, "ja");
        let (_, body) = render(&notice, "ja");
        assert_eq!(body.matches("https://").count(), LISTED);
        assert!(body.ends_with("ほか3件"), "{body}");
        let first = body.lines().next().unwrap();
        assert!(first.ends_with("…)"), "{first}");
        assert_eq!(
            render(&summary("k", &codes, "en"), "en").1.lines().last(),
            Some("and 3 more")
        );
    }

    /// Only a route that took the summary counts: a failed Webhook POST is not "told".
    #[tokio::test]
    async fn a_failed_webhook_is_not_a_delivery_but_the_app_inbox_is() {
        use crate::dashboard::{
            db::entities::{app_device, notification_setting, user},
            hoyolab::credentials::CookieKey,
            notifications::secret,
            test_support::TestDatabase,
        };
        use chrono::Utc;
        use sea_orm::{ActiveModelTrait, EntityTrait, Set};
        use std::sync::Arc;

        let fixture = TestDatabase::new().await;
        let db = &fixture.db;
        let key = Arc::new(CookieKey::from_hex(&"11".repeat(32)).unwrap());
        let notifications =
            Notifications::new(db.clone(), Some(key.clone()), "https://ahd.example.test");
        let owner = user::ActiveModel {
            id: Set(Uuid::new_v4()),
            username: Set("webhook-only".to_owned()),
            password_hash: Set("synthetic-private-hash".to_owned()),
            is_admin: Set(false),
            active: Set(true),
            must_change_password: Set(false),
            created_at: Set(Utc::now()),
            last_login_at: Set(None),
        }
        .insert(db)
        .await
        .unwrap()
        .id;
        // A Webhook nobody answers: a loopback port that was just closed (sealed directly,
        // past the public-host rule, so the test needs no network).
        let closed = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = closed.local_addr().unwrap().port();
        drop(closed);
        let url = format!("https://127.0.0.1:{port}/hook");
        notification_setting::ActiveModel {
            user_id: Set(owner),
            webhook_url: Set(Some(
                secret::seal(&key, secret::Purpose::Webhook, owner, &url).unwrap(),
            )),
            webhook_format: Set("json".to_owned()),
            language: Set("en".to_owned()),
            events: Set(serde_json::json!({})),
            last_delivery_at: Set(None),
            last_delivery_ok: Set(None),
            updated_at: Set(Utc::now()),
            resin_warn_hours: Set(2),
            daily_reminder_time: Set("21:00".to_owned()),
            weekly_reminder_time: Set("15:00".to_owned()),
            app_events: Set(serde_json::json!({})),
        }
        .insert(db)
        .await
        .unwrap();
        let codes = [code("genshin", "GENSHINGIFT", "")];
        assert!(
            !notifications
                .announce_codes(owner, "codes:1", &codes)
                .await
                .unwrap()
        );
        let row = notification_setting::Entity::find_by_id(owner)
            .one(db)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.last_delivery_ok, Some(false), "the Webhook was tried");
        // The same key is never tried twice; a later run uses its own key.
        assert!(
            !notifications
                .announce_codes(owner, "codes:1", &codes)
                .await
                .unwrap()
        );
        app_device::ActiveModel {
            id: Set(Uuid::new_v4()),
            user_id: Set(owner),
            created_at: Set(Utc::now()),
            last_seen_at: Set(Utc::now()),
            push_subscription: Set(None),
        }
        .insert(db)
        .await
        .unwrap();
        assert!(
            notifications
                .announce_codes(owner, "codes:2", &codes)
                .await
                .unwrap()
        );
        assert!(
            !notifications
                .announce_codes(owner, "codes:3", &[])
                .await
                .unwrap()
        );
    }
}

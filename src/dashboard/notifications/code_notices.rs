//! The notify-only summary of new redemption codes: one notice per user and fetch run.
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

impl Notifications {
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

/// The notice text: one line per code (game, code, rewards) and its redemption link.
/// `*` in source reward text means "times" and is shown as `×` (Discord reads `*` as markup).
pub(super) fn summary(key: &str, codes: &[NewCode], language: &str) -> Notice {
    let mut lines: Vec<String> = codes
        .iter()
        .take(LISTED)
        .map(|code| {
            let mut rewards: String = code.rewards.replace('*', "×");
            if rewards.chars().count() > REWARD_CHARS {
                rewards = rewards.chars().take(REWARD_CHARS - 1).collect::<String>() + "…";
            }
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

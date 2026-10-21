//! Server-side notification text in each channel's language.
use super::Notice;

/// Title and body for a notice; `{name}` placeholders take the notice's parameters.
pub(super) fn render(notice: &Notice, language: &str) -> (String, String) {
    let english = language == "en";
    let (title, body) = match (notice.kind, english) {
        ("test", false) => ("テスト通知", "AHDからの通知を受け取れます。"),
        ("test", true) => ("Test notification", "Notifications from AHD arrive here."),
        ("reward_failed", false) => (
            "ログインボーナスを受け取れませんでした",
            "{game}: {state}。実行履歴を確認してください。",
        ),
        ("reward_failed", true) => (
            "Login bonus not claimed",
            "{game}: {state}. Check the run history.",
        ),
        ("reward_claimed", false) => ("ログインボーナスを受け取りました", "{game}"),
        ("reward_claimed", true) => ("Login bonus claimed", "{game}"),
        ("cookie_invalid", false) => (
            "HoYoLABのCookieが使えなくなりました",
            "HoYoLAB連携から新しいCookieを登録してください。自動受け取りと状態取得は止まっています。",
        ),
        ("cookie_invalid", true) => (
            "HoYoLAB Cookie no longer works",
            "Register a new Cookie under HoYoLAB link. Automatic claiming and status updates have stopped.",
        ),
        ("signed_in", false) => (
            "ログインがありました",
            "{method}でログインしました。心当たりがなければパスワードを変更してください。",
        ),
        ("signed_in", true) => (
            "New sign-in",
            "Signed in with {method}. If this was not you, change your password.",
        ),
        ("sign_in_failed", false) => (
            "ログインに失敗しました",
            "パスワードが一致しませんでした。心当たりがなければパスワードを変更してください。",
        ),
        ("sign_in_failed", true) => (
            "Failed sign-in",
            "A password did not match. If this was not you, change your password.",
        ),
        ("resource_near_cap", false) => (
            "{resource}がまもなく上限です",
            "{game} UID {uid}: {current}/{max}。{at}に上限に達します。",
        ),
        ("resource_near_cap", true) => (
            "{resource} is nearly full",
            "{game} UID {uid}: {current}/{max}, full at {at}.",
        ),
        ("resource_full", false) => (
            "{resource}が上限に達しました",
            "{game} UID {uid}: {current}/{max}。これ以上は回復しません。",
        ),
        ("resource_full", true) => (
            "{resource} is full",
            "{game} UID {uid}: {current}/{max}. It no longer recovers.",
        ),
        ("expeditions_done", false) => (
            "派遣がすべて完了しました",
            "{game} UID {uid}: {count}件の報酬を受け取れます。",
        ),
        ("expeditions_done", true) => (
            "Expeditions are done",
            "{game} UID {uid}: {count} reward(s) are ready.",
        ),
        ("transformer_ready", false) => ("参量物質変化器が使えます", "原神 UID {uid}"),
        ("transformer_ready", true) => (
            "Parametric Transformer is ready",
            "Genshin Impact UID {uid}",
        ),
        ("daily_unfinished", false) => (
            "デイリーがまだ終わっていません",
            "{game} UID {uid}: {progress}",
        ),
        ("daily_unfinished", true) => ("Dailies are not done yet", "{game} UID {uid}: {progress}"),
        ("weekly_unfinished", false) => (
            "週間項目が残っています",
            "{game} UID {uid}: {items}。週の切り替わり前に済ませましょう。",
        ),
        ("weekly_unfinished", true) => (
            "Weekly tasks remain",
            "{game} UID {uid}: {items}. Finish them before the weekly reset.",
        ),
        ("user_attention", false) => ("ユーザーの対応が必要です", "{username}: {reason}"),
        ("user_attention", true) => ("A user needs attention", "{username}: {reason}"),
        ("worker_stalled", false) => (
            "自動受け取りの処理が止まっています",
            "報酬ワーカーが{minutes}分以上動いていません。サーバーのログを確認してください。",
        ),
        ("worker_stalled", true) => (
            "Reward worker stalled",
            "The reward worker has not run for over {minutes} minutes. Check the server logs.",
        ),
        ("deletion_stuck", false) => (
            "ユーザー削除が完了していません",
            "{username}の削除要求から1時間以上たっても完了していません。",
        ),
        ("deletion_stuck", true) => (
            "User deletion not finished",
            "Deleting {username} has not finished after an hour.",
        ),
        ("admin_sign_in_failed", false) => (
            "ログインに失敗しました",
            "{username}のパスワードが一致しませんでした。",
        ),
        ("admin_sign_in_failed", true) => {
            ("Failed sign-in", "A password for {username} did not match.")
        }
        ("admin_signed_in", false) => (
            "ログインがありました",
            "{username}が{method}でログインしました。",
        ),
        ("admin_signed_in", true) => ("New sign-in", "{username} signed in with {method}."),
        (_, false) => ("AHDからの通知", ""),
        (_, true) => ("AHD notification", ""),
    };
    (fill(title, &notice.params), fill(body, &notice.params))
}

/// Localized words used inside parameters.
pub(super) fn word(key: &str, language: &str) -> &'static str {
    let english = language == "en";
    match (key, english) {
        ("genshin", false) => "原神",
        ("genshin", true) => "Genshin Impact",
        ("starrail", false) => "崩壊：スターレイル",
        ("starrail", true) => "Honkai: Star Rail",
        ("resin", false) => "天然樹脂",
        ("resin", true) => "Original Resin",
        ("stamina", false) => "開拓力",
        ("stamina", true) => "Trailblaze Power",
        ("failed", false) => "失敗",
        ("failed", true) => "failed",
        ("unknown", false) => "結果不明",
        ("unknown", true) => "outcome unknown",
        ("action_required", false) => "HoYoLABで追加の確認が必要",
        ("action_required", true) => "HoYoLAB needs extra verification",
        ("password", false) => "パスワード",
        ("password", true) => "a password",
        ("passkey", false) => "パスキー",
        ("passkey", true) => "a passkey",
        ("weekly_boss", false) => "週ボス",
        ("weekly_boss", true) => "weekly bosses",
        ("echo_of_war", false) => "歴戦余韻",
        ("echo_of_war", true) => "Echo of War",
        ("simulated_universe", false) => "模擬宇宙",
        ("simulated_universe", true) => "Simulated Universe",
        ("cookie_invalid", false) => "Cookieが使えなくなりました",
        ("cookie_invalid", true) => "the HoYoLAB Cookie stopped working",
        ("reward_attention", false) => "ログインボーナスの結果確認が必要です",
        ("reward_attention", true) => "a login bonus result needs checking",
        (_, false) => "",
        (_, true) => "",
    }
}

fn fill(template: &str, params: &[(&'static str, String)]) -> String {
    let mut out = template.to_owned();
    for (name, value) in params {
        out = out.replace(&format!("{{{name}}}"), value);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_has_text_in_both_languages_with_parameters_filled() {
        let notice = Notice::new("resource_full", "k")
            .with("resource", "天然樹脂")
            .with("game", "原神")
            .with("uid", "800000001")
            .with("current", "200")
            .with("max", "200");
        let (title, body) = render(&notice, "ja");
        assert_eq!(title, "天然樹脂が上限に達しました");
        assert_eq!(
            body,
            "原神 UID 800000001: 200/200。これ以上は回復しません。"
        );
        for kind in super::super::PERSONAL_EVENTS
            .iter()
            .map(|(kind, _)| *kind)
            .chain([
                "test",
                "user_attention",
                "worker_stalled",
                "deletion_stuck",
                "admin_sign_in_failed",
                "admin_signed_in",
            ])
        {
            for language in ["ja", "en"] {
                let (title, _) = render(&Notice::new(kind, "k"), language);
                assert!(!title.starts_with("AHD"), "{kind} {language}");
            }
        }
    }
}

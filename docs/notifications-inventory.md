# 通知の一覧とAndroidアプリへの移植状況

web版が出しているすべての通知について、発生条件・内容・受信者と、Androidアプリへ移植したか（管理者専用として除外したか）をまとめます。通知の条件・文面の正本はサーバーの `src/dashboard/notifications/`（`conditions.rs`・`worker.rs`・`messages.rs`）です。

## 1. 配信経路

| 経路 | 受信者 | 実装 | アプリ |
| --- | --- | --- | --- |
| Webhook（個人） | 各ユーザー | `notifications/delivery.rs`。Discord形式またはJSON。設定 › 通知でURL・形式・言語・出来事を選ぶ | サーバー側の機能なので、アプリからも設定できる（従来どおり） |
| ブラウザのプッシュ通知（Web Push） | — | 削除済み（`push.rs`・`push_subscriptions`・`push-sw.js`・`/me/notifications/push`）。同じ通知はWebhookとアプリの受信箱で届く。登録済みのブラウザは、web版を開いたときに古いService Workerの登録を解除する（`dashboard-ui/src/retired_push.rs`） | — |
| Webhook（管理者） | 管理者チャンネル（1つ） | `notifications/channel.rs` の管理者チャンネル。管理者設定で設定 | 管理者専用のため除外 |
| アプリの受信箱（今回追加） | 各ユーザー（登録したアプリ） | `notifications/app.rs`。Webhookと同じ箇所（`Notifier::deliver`）で、同じ通知を受信箱へ入れる。アプリがポーリングして端末に表示する | 移植先 |
| 画面内の表示 | 操作中のユーザー | 保存完了やエラーなどの1行表示（`set_status`）と各画面の状態表示 | web版と同じコードなので、アプリにもそのままある。通知機能ではないので対象外 |

メール、SMS、ブラウザ内のトースト通知（Notification APIを直接使うもの）はありません。

## 2. 個人の通知（すべて移植）

受信者は、その出来事に関係するユーザー本人です。「既定」は通知設定を保存していないときの選択状態です。アプリは「通知する内容」の選択（Webhookと共通）に従い、Webhookと同じ通知だけを、同じ重複排除キーで受け取ります（Webhook URLが未設定でも届きます）。

| 種類（`kind`） | 発生条件 | 内容（日本語の例） | 既定 | アプリでタップしたときの画面 |
| --- | --- | --- | --- | --- |
| `reward_failed` | ログインボーナスの自動・手動実行が `failed`・`stopped`・`unknown`・`action_required` で終わった（実行ごとに1回） | 「ログインボーナスを受け取れませんでした」「{ゲーム}: {失敗/結果不明/HoYoLABで追加の確認が必要}。実行履歴を確認してください。」 | ON | 設定 › 報酬 |
| `reward_claimed` | 実行が `success`・`already_claimed` で終わった（実行ごとに1回） | 「ログインボーナスを受け取りました」「{ゲーム}」 | OFF | 設定 › 報酬 |
| `cookie_invalid` | HoYoLAB連携の状態が `cookie_invalid` になった（直近1日以内に更新された連携。Cookieの版ごとに1回） | 「HoYoLABのCookieが使えなくなりました」「HoYoLAB連携から新しいCookieを登録してください。…」 | ON | 設定 › 連携 |
| `signed_in` | 本人のアカウントへログインがあった（監査ログ `signed_in`、1件ごと） | 「ログインがありました」「{パスワード/パスキー}でログインしました。…」 | OFF | 設定 › 認証・セキュリティ |
| `sign_in_failed` | 本人のアカウントでパスワードが一致しなかった（監査ログ `sign_in_failed`、1件ごと） | 「ログインに失敗しました」「パスワードが一致しませんでした。…」 | ON | 設定 › 認証・セキュリティ |
| `resource_near_cap` | 天然樹脂・開拓力が上限未満で、上限到達まで「警告する時間」（1〜24時間、既定2時間）以内。使って警告時間の外に戻ると再び通知できる | 「{資源}がまもなく上限です」「{ゲーム} UID {uid}: {現在}/{上限}。{日時}に上限に達します。」 | ON | ダッシュボード |
| `resource_full` | 天然樹脂・開拓力が上限に達した（または予測時刻を過ぎた）。上限未満に戻ると再び通知できる | 「{資源}が上限に達しました」「{ゲーム} UID {uid}: {上限}/{上限}。これ以上は回復しません。」 | ON | ダッシュボード |
| `expeditions_done` | 実行中の派遣がすべて完了した。未完了が出ると再び通知できる | 「派遣がすべて完了しました」「{ゲーム} UID {uid}: {件数}件の報酬を受け取れます。」 | ON | ダッシュボード |
| `transformer_ready` | 参量物質変化器が使用可能になった（原神）。使用後に再び通知できる | 「参量物質変化器が使えます」「原神 UID {uid}」 | ON | ダッシュボード |
| `daily_unfinished` | サーバーのタイムゾーンで「デイリーの通知時刻」（既定21:00）を過ぎ、デイリー（原神の依頼、スターレイルの活躍度）が未完了。1日1回 | 「デイリーがまだ終わっていません」「{ゲーム} UID {uid}: {完了}/{全体}」 | ON | ダッシュボード |
| `weekly_unfinished` | 日曜の「週間の通知時刻」（既定15:00）以降に、週ボスの割引・歴戦余韻・模擬宇宙が残っている。ISO週ごとに1回 | 「週間項目が残っています」「{ゲーム} UID {uid}: {項目}。週の切り替わり前に済ませましょう。」 | ON | ダッシュボード |
| `test` | 設定 › 通知の「テスト送信」、またはアプリの「サーバーからテスト通知を送る」 | 「テスト通知」「AHDからの通知を受け取れます。」 | — | 設定 › 通知 |

ゲームの状態による通知（`resource_*`・`expeditions_done`・`transformer_ready`・`daily_unfinished`・`weekly_unfinished`）は、HoYoLAB連携が `verified` で、表示中かつ検出済みのゲームアカウントが対象です。サーバーは1分ごとに条件を評価し、状態は15分より古ければ取り直します。この取得は、Webhook・アプリのいずれかがあるユーザーだけに行います。

アプリでは、上の表のとおり通知を扱う画面を開きます。

## 3. 管理者専用の通知（除外）

管理者チャンネルのWebhookだけに送られ、受信者は管理者チャンネル（サーバーに1つ）です。個人の受信箱には送られません。アプリは管理者機能を持たない（[android-changes.md](android-changes.md) 3節）ため、移植していません。管理者は引き続き管理者設定のWebhookで受け取れます。

| 種類 | 発生条件 | 内容 | 除外の理由 |
| --- | --- | --- | --- |
| `user_attention` | いずれかのユーザーのログインボーナス実行が失敗・停止・結果不明・追加確認必要で終わった | 「ユーザーの対応が必要です」「{ユーザー名}: ログインボーナスの結果確認が必要です」 | 管理者専用（本人には `reward_failed` が届く） |
| `worker_stalled` | 報酬ワーカーが一定時間（15分単位）動いていない | 「自動受け取りの処理が止まっています」 | 管理者専用（サーバー運用の通知） |
| `deletion_stuck` | ユーザー削除の要求から1時間以上たっても完了していない | 「ユーザー削除が完了していません」 | 管理者専用 |
| `admin_sign_in_failed`（設定名 `sign_in_failed`） | いずれかのユーザーでパスワードが一致しなかった | 「ログインに失敗しました」「{ユーザー名}のパスワードが一致しませんでした。」 | 管理者専用（本人には `sign_in_failed` が届く） |
| `admin_signed_in`（設定名 `signed_in`） | いずれかのユーザーがログインした | 「ログインがありました」「{ユーザー名}が{方式}でログインしました。」 | 管理者専用（本人には `signed_in` が届く） |
| `test`（管理者） | 管理者設定の「テスト送信」 | 「テスト通知」 | 管理者専用 |

## 4. アプリでの受け取り方

### サーバー側（`src/dashboard/notifications/app.rs`）

- アプリは、通知をONにしたときに `POST /api/v1/me/notifications/app-devices` で端末を登録します（1ユーザー10台まで）。応答は端末IDと、読み始める位置（登録前の通知は届けない）です。
- `Notifier::deliver` は、Webhookへ送るのと同じ通知を、登録済みの端末があるユーザーの受信箱（`app_notifications`）へ入れます。文面は通知設定の言語で作り、重複排除キーをタグとして保存します。発生条件・選択・重複排除はWebhookと完全に同じ処理を通ります。
- アプリは `GET /api/v1/me/notifications/app-devices/{id}/inbox?after={位置}` で新しい通知を読みます（1回50件まで）。読むたびに端末の最終確認時刻を更新します。
- 受信箱の通知は3日、30日間読みに来ない端末は削除します（毎時の保守処理）。通知をOFFにすると `DELETE /api/v1/me/notifications/app-devices/{id}` で登録を消します。
- FCMなどの外部プッシュサービスは使いません。サーバーに秘密の資格情報を置かずに済み、Googleのサービスがない端末でも動きます。その代わり、届くまでに最大15分程度（Dozeでさらに遅れることがある）かかります。

### アプリ側

- `AhdInbox`（`dashboard-ui/android/MainActivity.kt`）が、アプリ専用領域に保存したセッションCookieで受信箱を読み、Androidの通知として表示します。JobSchedulerで15分ごと（ネットワーク接続時、アプリを閉じていても、再起動後も）に読み、アプリの表示中は1分ごとと、アプリに戻ったときにも読みます。
- 通知の表示・タップは端末通知と同じ仕組みです。チャンネル「ゲームの状態」を使い、タップするとアプリを前面に出して、2節の表の画面を開きます。同じタグの通知は置き換えます。
- 通知の許可：「この端末に通知する」をONにしたとき、Android 13以降で許可がなければ `POST_NOTIFICATIONS` を求めます。拒否された場合は警告と「通知を許可する」「端末の通知設定を開く」を表示します。
- アプリの予測通知（樹脂・開拓力・洞天宝銭の上限、派遣、参量物質変化器。[android-changes.md](android-changes.md) 5.2節）は残しています。同じ状態について、予測通知とサーバーの通知の両方が2時間以内に届いた場合は、後から来たほうを表示しません（タグ「ゲームアカウント:種類」が一致するもの）。
- ログアウトやセッションの失効で、端末の受信を止めます。サーバー側の登録は30日後に消えます。別のユーザーでログインすると、そのユーザーで登録し直します。

## 5. 検証

- `src/dashboard/notifications/worker_tests.rs`（隔離したPostgreSQL）
  - 1回の評価で、個人の通知10種類（`test` 以外）がすべて受信箱に入り、そのタグの集合がWebhookの配信記録（`notification_log`）と一致すること。文面・タグ・開く画面。2回目の評価では増えないこと。
  - 既定の選択（`reward_claimed`・`signed_in` はOFF）に従い、端末を登録していないユーザーには入らないこと。
  - `cookie_invalid` がCookieの版ごとに1回だけ入ること。
  - 端末の所有者の確認、10台の上限、登録解除、テスト送信、古い通知と端末の削除。
- `src/dashboard/http/tests/notifications.rs`：Cookie認証での登録（Originが必要）・テスト送信・受信箱の読み取り（GETはOrigin不要）・解除。
- `src/dashboard/notifications/app.rs`：種類ごとの開く画面。`dashboard-ui/src/mobile/inbox.rs`：登録・継続・解除の判断。`dashboard-ui/src/app/navigation.rs`：通知の行き先から画面への対応。
- エミュレータ（API 34）での手動確認：`resource_full`・`daily_unfinished`（アプリ表示中）、`sign_in_failed`・`reward_failed`・`cookie_invalid`（アプリを前面に出していない状態で定期ジョブから表示）、タップで開く画面、通知の許可の要求、OFF/ONでの登録解除と再登録。結果は [android-changes.md](android-changes.md) 5.3節にあります。

## 6. web版への影響と後続作業

- web版のWebhook以外の通知（Web Push）は削除しました。サーバーは配信時と「テスト送信」でWebhookとアプリの受信箱だけに送ります。テスト送信の応答は `{webhook, app}` です。
- 削除したもの：`notifications/push.rs`、テーブル `push_subscriptions`（マイグレーション `m20261028_000010_remove_web_push` で削除。downで元に戻せる）、`assets/push-sw.js`、`PushCard`、`POST/DELETE /me/notifications/push`、`GET /me/notifications` の `push_public_key`・`push_subscriptions`。
- 残したもの：通知の評価（`worker.rs`・`conditions.rs`・`messages.rs`）、`notification_settings`、`app.rs` と `/me/notifications/app-devices`（アプリが使う）、Webhook（個人・管理者）。
- 管理者専用の通知（3節）は、もともと管理者チャンネルのWebhookだけに送られていたので、そのまま残しました。Web Pushには送られていなかったため、管理者が受け取れなくなる通知はありません。
- v1.4.0のアプリは `GET /me/notifications` の `push_subscriptions` を必須として読むため、このサーバーでは「通知」ページのWebhook設定を読み込めません。アプリをv1.5.0へ更新してください（端末の予測通知は影響を受けません）。

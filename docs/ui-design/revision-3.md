# AGD R3 — 指摘の反映

2026-10-15。AI制作の編集用ドラフト。最終承認は人間。本番Dioxus／API／DBは変更していない。

## UIの変更

- 通常画面の補助説明、ログインの左紹介欄とボタン下の文言を削除。操作名・項目名・データ・エラー／受付状態は残す。確認用バナー／footerは製品外で、Nativeへ取り込まない。
- ログインは中央の幅480pxカード1枚。共通ユーザー名を維持し、パスワード欄と送信の間に24px以上の余白。
- Desktop Dashboardは原神とスターレイルを同時に等幅表示。Mobileは下線式ゲームタブで切替。カード全面は緑／紫、それぞれのカード上部にログインボーナスON/OFF。別ボーナスカードは廃止。自動化は引き続きアカウント×ゲームで、UID選択に依存しない。
- メニューは文字のない簡素なハンバーガー。管理者設定の項目は管理者fixtureだけに表示。
- 初期登録は新パスワード2欄→Cookie→本人確認。再発行後は同じ隔離シェル内に新パスワード2欄のみで、Cookie欄・Cookie段階を出さない。どちらも現在のパスワード入力欄を置かない。
- Settingsは内容切替の5タブ。Desktopは完全な名前、Mobileは「ゲーム／ボーナス／連携／認証／削除」。アクセシブル名は省略せず、矢印・Home・End・roving tabindexに対応。単なるスクロール目次ではない。
- 時刻は大きな「時」「分」のselect。任意の1分単位に対応し、下のタイムゾーン`+09:00`を維持。直近5日の実行履歴・queued／unknownの区別も残す。
- 管理者の作成／再発行フォームから仮パスワード指定欄を削除。生成後の受渡し画面を別状態として作成。表示値は固定の明白な合成値であり、実際の生成・clipboard・API送信は実装していない。
- 表示ラベルを「ユーザー削除」に統一。Settingsはログイン中本人、管理者一覧は指定した対象という既存認可境界を変更しない。対象名の一致・実行者の現在パスワード欄を残す。削除受付を完了と扱わない。

## 現在の契約と、未実装の差分

| 要望 | 実契約／実装前の確認事項 |
|---|---|
| 必須パスワード変更で現在値の入力なし | `PUT /api/v1/me/password`は今も`current_password`と`new_password`が必須。専用方針の検討が必要。ログインパスワードを保持して隠し送信する実装は追加していない。 |
| 再発行後にCookie欄なし | 再発行は既存HoYoLAB連携を消さないため、検証済み連携が残る利用者はパスワード変更だけで既存ゲートを満たせる。未登録・無効Cookie・暗号化設定不足の場合までゲートを迂回する契約ではない。APIには初回／再発行の理由flagはない。R3のrecovery fixtureは既存連携完了という合成例。 |
| 自動生成の仮パスワード | 現在の管理者作成／再発行は呼出側が`temporary_password`を指定し、生成秘密のレスポンスはない。生成主体、暗号学的生成、安全な受渡し・再表示方針は未確定。クライアント生成なら現行request形状を利用できるが、R3はそれも実装していない。 |
| ユーザー削除 | `/me`と管理者の`/admin/users/{id}`の認可は維持。名称変更で一般利用者に他人の削除権限を追加しない。 |

根拠：`src/dashboard/http/auth.rs`、`http/users.rs`、`auth/accounts.rs`、`auth/deletion.rs`、`http/hoyolab.rs`、`dashboard-ui/src/app/state.rs`、`docs/dashboard-api.json`。管理者ログ取得APIは未提供のまま。

## Penpot保存内容

既存ファイル`129977ce-683b-806a-8008-c9d0bedd3212`。R1とR2（Desktop／Mobile各25ボード）は維持し、R3を別ページ／Assetsに追加した。

| ページ | ID | 内容 |
|---|---|---|
| AGD R3 Draft / Desktop | `14c6bf6c-f6fc-8051-8008-cb15e12f9a86` | 21場面 |
| AGD R3 Draft / Mobile | `14c6bf6c-f6fc-8051-8008-cb17d2cb3637` | 22場面（ゲーム切替追加） |
| AGD R3 Draft / Components | `14c6bf6c-f6fc-8051-8008-cb15dcde7367` | 共通部品25種類、色・寸法tokensとTypography assets |

Native Board／Text／ベクトル／componentコピー。PNG貼付ではない。今回は製品画面へ説明ボードを追加していない。

`r2-import.js`は検証済みscene sourceのR2／R3を判別して別scopeへ取り込む共通処理。既存キー、storage喪失時の既存Assets、途中マーカーで盲目的な再実行を止める。`r2-refresh.js`も同じ2つのscopeに限定し、コピー文字の空白化→復元を別RPCで行う。R3はDesktop191文字／Mobile194文字を処理・復元した。

`r3-finish.js`で代表導線171件、Login／Initial Setup／Recovery／Daily／Adminの開始Flowを各viewportへ追加。既存人間リンクを上書きしない。フォーム、実秘密生成、clipboard、switch全組合せ、初回非表示UID、管理者roleを引き継ぐ通常Dashboard／Settingsの全組合せは再現していない。存在しないAPIや未キャプチャの結果へはリンクしない。

## 検証と限界

- ブラウザ43場面の配置／入力秘匿／44px領域／色付きカードcontrast、タブ選択とkeyboard、初回と再発行の差、管理者の権限表示・生成結果mock、時刻保存／timezoneを検査。
- 独立担当が12 PNGを批評し、Mobileタブの不自然な改行を指摘。短い表示名へ1回の修正を行い再検査。実行済みSkill：ImpeccableのOperate／Distill／craft-floor、UI UX Pro Maxの既存Quick Reference（Python CLI成功の主張はしない）。
- Native中央Loginを試験取り込み・exportしてから全体へ展開。全43キーのsource hashは現在の4つのpreview sourceと一致。最終検査：再発行画面のCookie／現在パスワード0、一般fixtureの管理者項目0、disabled操作へのリンク0、途中マーカーなし、**ファイル検証エラー0**。
- Native SVGのResourceRequest／timeoutが複数あった。取得できた完全SVGをPNGへ描画し、別途直接PNG exportでDesktop／Mobile DashboardとMobile時刻設定を確認。中央Login、初期設定、再発行後、生成結果、ユーザー削除タブも画像確認した。生成画像は`/tmp/`のみ。
- 全43場面の最終画像批評、可変Auto Layoutの全面整備、全操作・支援技術の実機検査は未完了。R2からの制約として、SDKでの色token／Typography値変更だけで既存図形の自動同期を保証しない。本番認可・実アカウント操作の証明ではない。

再検査：`node docs/ui-design/preview/check.mjs`で`/tmp/agd-r3`へ生成後、`node docs/ui-design/penpot/r2-check.mjs`で共通importのR2隔離・R3の43場面／導線／復元を限定模擬検査する。`node docs/ui-design/check.mjs`は歴史的R1の構文／配色検査であり、R3の視覚合格ではない。生成物・認証設定・秘密はコミットしない。

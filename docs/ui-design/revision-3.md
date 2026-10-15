# AGD R3 — 指摘の反映

2026-10-15。AI制作の編集用ドラフト。R3を本実装の基準としてユーザー承認済み。この記録はデザイン作業単位であり、本番実装は別作業。

## 最新：同じR3内の小改修（R4ではない）

- 全パスワード入力に48pxの目を追加。入力label／ID、`aria-controls`・`aria-pressed`・日英の表示／非表示名を持つ。値は空・read-onlyの合成例のまま、typeのみ切替。値getterは使わず、計測も`data-password`で識別するためtype=textでも値を読まない。再render／window blur／document hidden／メニューを開く時に再マスクする。
- 英語は製品画面全体に対応（`preview/i18n.js`）。日本語／英語・dark／lightの操作は**メニュー内**。言語は閉じず即時反映し、テーマは背面にも反映。`localStorage`は`agd-preview-lang`／`agd-preview-theme`のみ、認証情報・表示状態は保存しない。URLのlang／themeは合成キャプチャ用。ログイン／制限中の初期設定にはメニューを出さず保存済み設定を継承し、完了前のログアウトのみのゲートを維持する。
- 公開参照`https://artifact-diagnoser.ktonk.com/`を実Chromiumの1440×1000／390×844で操作・観察（担当`01a13f78-baf6-70b4-b4f6-59df54c604d8`、`/tmp/agd-menu-reference/`）。右端全高304px、暗いbackdrop、本文移動なし、右から約300ms、外側／Escapeで閉じる。参照はgear・Xなし。AGDはハンバーガーを維持し明示Xを追加、native dialogのフォーカス管理、外側／Escape、280ms transform＋discrete close、reduced-motionに対応。参照ソースの正確なduration／easingを調べた主張ではない。

### 今回の検査・保存状況

- ブラウザ検査成功：`/tmp/agd-r3/manifest.json`を正本に59 JSON＋59 PNG。当初43＋代表16（両幅でDashboard／menuのen-dark・ja-light・en-light、Login／Settings認証のen-light）。表示切替と再マスク／秘密非読取、英語、保存設定、フォーカス復帰、drawer寸法・本文非移動・入場motion、カードcontrastと既存R3責務を検査した。
- 限定模擬も成功：59場面・模擬common map50件、R2／人間編集の隔離、重複／中断ガード、メニュー置換のsnapshot／board ID維持、目の冪等追加、設定の代表リンク、2 RPC文字復元。**map50はNativeの最終部品数ではなく、実API／描画の証明でもない。** 今回読んだSkillはImpeccable harden／craft-floorとUI UX Pro Max Quick Reference。Python CLI成功とは扱わない。
- Nativeは目パッチのpreflight／実行に成功し、既存12枚に静的な編集用の目20個＋別の共有masterを追加済み。元25共通Assetsは保持。既存メニュー4枚（一般／管理者×両幅）は子だけを置換しboard ID維持を確認。元の他39枚の本文は再生成していない（一部は目のみ追加）。R1／R2は未変更。
- 追加16枚もNativeへ保存。最終inventoryはDesktop29／Mobile30、共通map51＋目master、207リンク（102／105）。コピー文字266／263件を2 RPCで復元。import／目／文字の途中印なし、ファイル検証エラー0。旧Mobile Loginの目を直接PNGで確認。ライトの追加画像確認はユーザー指定で省略し、全画像批評完了とはしない。
- 新メニュー／代表場面のsource hashは`7e465ffc4bfd9b0e0e4205054ff4af67176ca49bf85cc744f480eb47aefeefbd`。非メニューの元本文は目の加算のみなので、元hash `a8d47ca179932ca81cd72079ca1a0982337c8e787c8b8591385529908cffb5cd`を意図的に保持。全既存ボードが最新source hashと一致するという受入条件ではない。
- Nativeの目は静的アイコンで秘密を表示しない。実際の表示切替・motion・設定永続化は合成ブラウザだけ。Nativeメニューは言語／テーマの代表状態へリンクできるが、全管理者role・ページ組合せは静的なまま。本番／実アカウント／認証器／上流操作は追加しておらず、下記API差分も未解消。

## 当初R3のUI変更（43場面の基準記録）

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

## 当初R3のPenpot保存内容（今回の最終数ではない）

既存ファイル`129977ce-683b-806a-8008-c9d0bedd3212`。R1とR2（Desktop／Mobile各25ボード）は維持し、R3を別ページ／Assetsに追加した。

| ページ | ID | 内容 |
|---|---|---|
| AGD R3 Draft / Desktop | `14c6bf6c-f6fc-8051-8008-cb15e12f9a86` | 21場面 |
| AGD R3 Draft / Mobile | `14c6bf6c-f6fc-8051-8008-cb17d2cb3637` | 22場面（ゲーム切替追加） |
| AGD R3 Draft / Components | `14c6bf6c-f6fc-8051-8008-cb15dcde7367` | 共通部品25種類、色・寸法tokensとTypography assets |

Native Board／Text／ベクトル／componentコピー。PNG貼付ではない。今回は製品画面へ説明ボードを追加していない。

`r2-import.js`は検証済みscene sourceのR2／R3を判別して別scopeへ取り込む共通処理。既存キー、storage喪失時の既存Assets、途中マーカーで盲目的な再実行を止める。`r2-refresh.js`も同じ2つのscopeに限定し、コピー文字の空白化→復元を別RPCで行う。R3はDesktop191文字／Mobile194文字を処理・復元した。

`r3-finish.js`で代表導線171件、Login／Initial Setup／Recovery／Daily／Adminの開始Flowを各viewportへ追加。既存人間リンクを上書きしない。フォーム、実秘密生成、clipboard、switch全組合せ、初回非表示UID、管理者roleを引き継ぐ通常Dashboard／Settingsの全組合せは再現していない。存在しないAPIや未キャプチャの結果へはリンクしない。

## 当初R3の検証と限界（今回の受入結果ではない）

- ブラウザ43場面の配置／入力秘匿／44px領域／色付きカードcontrast、タブ選択とkeyboard、初回と再発行の差、管理者の権限表示・生成結果mock、時刻保存／timezoneを検査。
- 独立担当が12 PNGを批評し、Mobileタブの不自然な改行を指摘。短い表示名へ1回の修正を行い再検査。実行済みSkill：ImpeccableのOperate／Distill／craft-floor、UI UX Pro Maxの既存Quick Reference（Python CLI成功の主張はしない）。
- Native中央Loginを試験取り込み・exportしてから全体へ展開。当初は全43キーのsource hashが当時の4つのpreview sourceと一致した（上記の元hash）。今回の追加修正後の全キー一致を意味しない。最終検査：再発行画面のCookie／現在パスワード0、一般fixtureの管理者項目0、disabled操作へのリンク0、途中マーカーなし、**ファイル検証エラー0**。
- Native SVGのResourceRequest／timeoutが複数あった。取得できた完全SVGをPNGへ描画し、別途直接PNG exportでDesktop／Mobile DashboardとMobile時刻設定を確認。中央Login、初期設定、再発行後、生成結果、ユーザー削除タブも画像確認した。生成画像は`/tmp/`のみ。
- 全43場面の最終画像批評、可変Auto Layoutの全面整備、全操作・支援技術の実機検査は未完了。R2からの制約として、SDKでの色token／Typography値変更だけで既存図形の自動同期を保証しない。本番認可・実アカウント操作の証明ではない。

現在の再検査：`node docs/ui-design/preview/check.mjs`で`/tmp/agd-r3`へ59場面を生成後、`node docs/ui-design/penpot/r2-check.mjs`で共通importのR2隔離・R3のメニュー置換／目追加／代表設定リンク／復元を限定模擬検査する。`node docs/ui-design/check.mjs`は歴史的R1の構文／配色検査であり、R3の視覚合格ではない。生成物・認証設定・秘密はコミットしない。

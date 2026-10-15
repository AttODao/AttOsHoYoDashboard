# AGD UIデザイン案

**最新は同じR3内の小改修（R4ではない）。ブラウザ59場面を検査済み。Native59場面を保存、R3を本実装の基準としてユーザー承認済み。本番Dioxus・API・DBは未変更。**

## 今回の成果物

- [`preview/index.html`](preview/index.html)：ブラウザで開ける画面／操作プレビュー。HTML・CSS・JavaScriptのデザイン資料であり、本番フロントエンドではない。
- [`revision-3.md`](revision-3.md)：最新指摘の反映、タブ切替・再発行後の分離、実APIとの差分、Native保存・検証範囲。
- [`revision-2.md`](revision-2.md)：旧R2の参照UI・構成・制作記録。
- [`DESIGN.md`](DESIGN.md)：R3の配色・タイポグラフィ・レイアウト。値の正本は`preview/style.css`。
- [`review.md`](review.md)：実際に行ったSkill利用・R3小改修の検査と限界、旧案／R2の履歴。
- [`preview/scene.mjs`](preview/scene.mjs)／[`penpot/r2-import.js`](penpot/r2-import.js)：合成プレビューの配置計測とNativeへの取り込み。R2／R3を別scopeへ取り込む共通処理。
- [`native-status.md`](native-status.md)：旧R2の保存・検証記録。最新R3は`revision-3.md`。
- [`penpot/r3-finish.js`](penpot/r3-finish.js)：設定／ゲームタブ、初期登録／再発行後の代表導線。
- [`penpot/r2-guide.js`](penpot/r2-guide.js)／[`penpot/r2-refresh.js`](penpot/r2-refresh.js)：Nativeガイド追加と、コピー文字の描画キャッシュを2回のRPCで更新する補助。

`preview/index.html`は直接開ける。Webフォントの読込にはインターネットが必要で、未読込時はフォールバックする。操作は合成状態の切替のみ。認証入力欄は読取専用で、実Cookie・パスワードを入力しない。全パスワード入力に表示／非表示の目アイコンを追加したが、値は空の合成例のまま。英語の製品画面に対応し、メニュー内で日本語／英語とdark／lightを切り替える。保存するのは`agd-preview-lang`／`agd-preview-theme`の2設定だけで、認証情報・表示状態は保存しない。下部の「デザイン確認用」は製品メニューではない。

## 新しい構成

- 表記は**AGD**。ユーザー指定の[NFT/Web3参照](https://www.uupm.cc/demo/nft-web3)を基に、黒い面、マゼンタ／シアン、角のある表示書体、余白を採用。マーケットプレイス機能・NFT画像・ウォレットは追加しない。
- **常設サイドバーなし**。上部のハンバーガーメニュー1つからHoYoLAB連携、設定、管理者設定（管理者のみ）、ログアウト。
- ダッシュボードはゲーム状態＋ゲームごとの自動受け取りON/OFF。Desktopは2ゲームを同時表示し、Mobileはタブ。各色付きカードの上部にON/OFFを置く。時刻、手動実行、履歴、UID表示、連携、セキュリティ、ユーザー削除は設定タブへまとめる。
- ログインは**共通ユーザー名＋1枚のフォーム**。中央に配置し、パスワード／パスキーで同じユーザー名を使う。説明欄は置かない。
- 仮登録は**独立した初期設定画面**。パスワード変更→Cookie保存→本人確認。完了前はログアウト以外の遷移を禁止し、本人確認成功後にダッシュボードへ移動する。新UIDは非表示なので初回はEmptyになり得る。再発行後の例はパスワード変更だけで、Cookie欄なし。現在値を入力しない必須変更は現行API未対応。
- 管理者設定は既存ユーザー管理を維持。ログ確認は要望の置き場所を用意するが、現行の取得APIがないため架空のログ・機能を動作済みとして示さない。

## Penpotの状態

**当初R3の保存済み基準は43場面・共通部品25種類・代表導線171件・名前付き10 Flow。主要画像／構造確認とファイル検証エラー0は当初の記録であり、今回の最終合格ではない。** 小改修では既存メニュー4枚の子だけを置換してboard IDを維持し、既存12枚へ静的な目20個＋共有masterを追加済み。元の25共通Assetsは保持。追加16代表場面も保存済み。合計59枚、共通map51＋目master、207リンク。コピー文字266＋263件を復元し、途中印なし・ファイル検証エラー0。追加ライト画像確認はユーザー指定により省略。接続設定は未変更。

保存済みのR1は同じファイル **New File 1** / `129977ce-683b-806a-8008-c9d0bedd3212` にある。R3は`AGD R3 Draft / Desktop`・`Mobile`・`Components`へ分離。今回もR1／R2と、メニュー以外の元39枚の本文を維持する（目の追加のみ一部あり）。

| 旧R1ページ | ID |
|---|---|
| `Draft / 00 / Design System` | `609d2656-00af-8046-8008-ca15c8b64a3a` |
| `Draft / 01 / Concepts` | `609d2656-00af-8046-8008-ca15c8b814d2` |
| `Draft / 02 / Desktop` | `609d2656-00af-8046-8008-ca15c8b8f352` |
| `Draft / 03 / Mobile` | `609d2656-00af-8046-8008-ca15c8b9979b` |
| `Draft / 04 / Components` | `609d2656-00af-8046-8008-ca15c8ba4085` |

R1の説明は[`revision-1.md`](revision-1.md)、旧制作スクリプトは`penpot/{bind,setup,concepts,screens,states,system,finish,inspect}.js`、旧トークンは`tokens.json`。これらは**R2の生成コードではない**。描画キャッシュと見出しの問題が残ったR1の最終状態を合格とは扱わない。`finish.js`をそのまま再実行しない。

## 検証コマンド

```sh
node docs/ui-design/check.mjs
node docs/ui-design/preview/check.mjs
node docs/ui-design/penpot/r2-check.mjs
```

2つ目は既存のPlaywright／Chromiumが必要。既存UIテストと同じ`ATTOS_TEST_PLAYWRIGHT_MODULE`でインストール済みモジュールを指定できる。キャプチャはデフォルトで`/tmp/agd-r3/`へ59 PNG・59計測JSON・manifestを保存する（当初43＋言語／テーマ代表16、一覧の正本はmanifest）。実書体の読込を要求し、未読込のfallback計測を成功扱いしない。生成PNG／JSONやMCP認証設定はコミットしない。

3つ目は2つ目の生成JSONを使う限定的な模擬テスト。59場面・模擬共通map50件、R2／人間編集の隔離、メニュー置換時のboard ID維持、目の追加の冪等性、設定の代表リンク、書体・配色・寸法の事前検査、中断ガード、再発行後のCookie遷移禁止と2 RPCのコピー文字復元を検査する。模擬map50はNativeの最終部品数ではない。**実PenpotのAPI互換性・描画・編集性は証明しない**。最新の保存状態・残る制約は[`revision-3.md`](revision-3.md)を参照。

プレビューの画面遷移・配置・表示規則の検査であって、実APIの認可、認証器、HoYoLAB契約、本番ブラウザアクセシビリティの証明ではない。最終RSX／CSS実装は別の指示で行う。

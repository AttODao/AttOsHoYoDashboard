# AGD UIデザイン案

**最新はR2。AIの編集用案・人間の承認前。本番Dioxus・API・DBは未変更。**

## 今回の成果物

- [`preview/index.html`](preview/index.html)：ブラウザで開ける画面／操作プレビュー。HTML・CSS・JavaScriptのデザイン資料であり、本番フロントエンドではない。
- [`revision-2.md`](revision-2.md)：参照UI、画面構成、初期設定の制約、管理者ログの未確定範囲、Penpot反映手順。
- [`DESIGN.md`](DESIGN.md)：R2の配色・タイポグラフィ・レイアウト。値の正本は`preview/style.css`。
- [`review.md`](review.md)：実際に行ったSkill利用・旧案の問題・R2の検証と限界。
- [`preview/scene.mjs`](preview/scene.mjs)／[`penpot/r2-import.js`](penpot/r2-import.js)：合成プレビューの配置計測とNativeへの取り込み。ユーザー許可後、24場面を実作成済み。
- [`native-status.md`](native-status.md)：保存できた場面、実検証と模擬検証の区別、再開手順・未完了項目。

`preview/index.html`は直接開ける。Webフォントの読込にはインターネットが必要で、未読込時はフォールバックする。操作は合成状態の切替のみ。認証入力欄は読取専用で、実Cookie・パスワードを入力しない。下部の「デザイン確認用」は製品メニューではない。

## 新しい構成

- 表記は**AGD**。ユーザー指定の[NFT/Web3参照](https://www.uupm.cc/demo/nft-web3)を基に、黒い面、マゼンタ／シアン、角のある表示書体、余白を採用。マーケットプレイス機能・NFT画像・ウォレットは追加しない。
- **常設サイドバーなし**。上部のハンバーガーメニュー1つからHoYoLAB連携、設定、管理者設定（管理者のみ）、ログアウト。
- ダッシュボードはゲーム状態＋ゲームごとの自動受け取りON/OFF。時刻設定、手動実行、履歴、UID表示、連携、セキュリティ、自己削除は設定ページへまとめる。
- ログインは**共通ユーザー名＋1枚のフォーム**。パスワード／パスキーで同じユーザー名を使う。
- 仮登録は**独立した初期設定画面**。パスワード変更→Cookie保存→本人確認。完了前はログアウト以外の遷移を禁止し、本人確認成功後にダッシュボードへ移動する。新UIDは非表示なので初回はEmptyになり得る。
- 管理者設定は既存ユーザー管理を維持。ログ確認は要望の置き場所を用意するが、現行の取得APIがないため架空のログ・機能を動作済みとして示さない。

## Penpotの状態

**ユーザーの再開許可後、通常のMCPでR2を24場面・共通部品20種類まで追加。現在はPenpotタブのheartbeat停止で中断。残り26場面と最終検証は未完了。** 接続設定の再調査・再変更は行っていない。

保存済みのR1は同じファイル **New File 1** / `129977ce-683b-806a-8008-c9d0bedd3212` にある。R2では既存ボードを上書きせず、別ページに展開する。

| 旧R1ページ | ID |
|---|---|
| `Draft / 00 / Design System` | `609d2656-00af-8046-8008-ca15c8b64a3a` |
| `Draft / 01 / Concepts` | `609d2656-00af-8046-8008-ca15c8b814d2` |
| `Draft / 02 / Desktop` | `609d2656-00af-8046-8008-ca15c8b8f352` |
| `Draft / 03 / Mobile` | `609d2656-00af-8046-8008-ca15c8b9979b` |
| `Draft / 04 / Components` | `609d2656-00af-8046-8008-ca15c8ba4085` |

R1の説明は[`revision-1.md`](revision-1.md)、旧制作スクリプトは`penpot/`の`r2-*`以外、旧トークンは`tokens.json`。これらは**R2の生成コードではない**。描画キャッシュと見出しの問題が残ったR1の最終状態を合格とは扱わない。`finish.js`をそのまま再実行しない。

## 検証コマンド

```sh
node docs/ui-design/check.mjs
node docs/ui-design/preview/check.mjs
node docs/ui-design/penpot/r2-check.mjs
```

2つ目は既存のPlaywright／Chromiumが必要。既存UIテストと同じ`ATTOS_TEST_PLAYWRIGHT_MODULE`でインストール済みモジュールを指定できる。キャプチャはデフォルトで`/tmp/agd-r2/`へ30 PNG・50計測JSON・入力秘匿の検査JSONを保存する。実書体の読込を要求し、未読込のfallback計測を成功扱いしない。生成PNG／JSONやMCP認証設定はコミットしない。

3つ目は2つ目の生成JSONを使う限定的な模擬テスト。50場面、共通操作部品、上書き防止、書体・配色・寸法の事前検査、途中失敗からの盲目的な再開防止、代表導線と初期設定の遷移制限を検査する。**実PenpotのAPI互換性・描画・編集性は証明しない**。Nativeへの反映手順と残る制約は[`revision-2.md`](revision-2.md#オフライン取り込み準備)を参照。

プレビューの画面遷移・配置・表示規則の検査であって、実APIの認可、認証器、HoYoLAB契約、本番ブラウザアクセシビリティの証明ではない。最終RSX／CSS実装は別の指示で行う。

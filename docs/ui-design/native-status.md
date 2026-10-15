# R2 / Native作業状況

最終記録：2026-10-15。**50場面＋Guideを保存した未承認ドラフト。人間の最終編集・承認は未実施。**

## 保存内容

ファイル`129977ce-683b-806a-8008-c9d0bedd3212`内に追加。旧R1の75ボードとPage 1を維持。本番Dioxus・API・DB・MCP接続設定は変更していない。

| ページ | ID | 内容 |
|---|---|---|
| AGD R2 Draft / Desktop | `07572e50-47e6-8015-8008-caced573b398` | 25場面 |
| AGD R2 Draft / Mobile | `07572e50-47e6-8015-8008-cad0ac5aac4f` | 25場面 |
| AGD R2 Draft / Components | `07572e50-47e6-8015-8008-cacea4b5005e` | 共通操作部品20種類 |
| AGD R2 Draft / Guide | ボード：`07572e50-47e6-8015-8008-cade432f3c60` | Native文字48＋コピー内文字4、色見本14、部品コピー4、論理グループ5 |

各viewportのキー：`login`、`dashboard`、`setup`、`settings`、`admin`、`admin-deleted`、`admin-deletion`、`admin-recovery`、`dashboard-new-uids`、`dashboard-on`、`dashboard-starrail`、`deleted`、`empty`、`error`、`loading`、`login-passkey`、`menu`、`menu-admin`、`passkey-cancel`、`permission`、`queued`、`setup-cookie`、`setup-pending`、`storage`、`unknown`。

Native Board／Text／ベクトル／部品コピー、tokensとTypography assetsであり、PNGを貼った画面ではない。Guideは独立した編集可能ボード。既存／途中のGuideを上書きしないガードを持つ。

## 実際に確認したこと

- 通常の`penpot-native`を使用。対象ファイル・R1・Noto Sans JP／Orbitronのweightを確認。別SDK bridgeや接続設定の再修復は使っていない。
- Native Gradientのsetter/getterと初期PNGを確認。SVG経由の変換は廃止。幅0の境界へstrokeColor tokenを付けず、trackingと段落幅も計測するよう修正。
- 主要10画面の初期export後、修正済み16枚をexport・PNG化。独立担当が14枚を画像レビューし、Dashboard／Settings／Admin両幅の閉じたメニュー漏れ解消、両Mobile展開メニューの前面描画、queued／unknown／管理者ダイアログ／自己削除受付の区別を確認した。親担当がGuideと管理者による削除受付（自身のsession維持）を追加確認した。
- 独立レビューで本人確認CTAの空欄を発見。getter上は正しい文言でもglyphが古かった。font-sizeの2RPC更新でCTAは復旧したが、Cookie欄はmasterの`demo_player`が残った。**文字を空白へ変更するRPC→元文言を復元するRPC**で修正し、再exportしたMobile本人確認待ち画像では「本人確認を実行」「実Cookieを入力しないでください」の両方を確認した。空文字はAPIが拒否するため空白を使う。
- `r2-refresh.js`で両viewportのコピー文字各256件＋Guide4件へこの2段階更新を適用し、元の文字・サイズ・fontSize tokenを復元。途中中断時は保存した復元記録を優先し、別ページの更新を拒否する。R1／非コピーの人間テキストには適用しない。
- ボタンmasterの一時fill変更がコピーへ反映されることを確認し、元のGradientへ復元。色tokenやTypographyのSDK値変更だけでは既存図形への自動反映を確認できなかった。Typographyの明示Applyで字間変更は確認できたが、全体の自動同期保証ではない。Guideにも制約を明記。
- 代表導線240件。Login／Setup／Daily／Adminの名前付き開始Flowを両viewportに作成（Penpotの自動Flowも別途存在）。フォーム入力、ページ内scroll、保存通知、全スイッチ組合せのシミュレーションではない。
- 最終読み取り：Desktop／Mobile各25場面、共通部品20、閉じたメニューの誤表示0、初期設定からSettings／Admin／Menuへの誤リンク0、disabled印付き操作のリンク0、AttOs文言0、**ファイル検証エラー0**。import／glyph／content／共有編集試験の途中マーカーもすべて空。
- 最後のunknown 2件のMCP応答timeoutは、キーの存在と`inProgress: null`で保存完了を確認し重複送信しなかった。タブ休止は前面化で再開。exportのResourceRequest／foreignObjectのHTTP 500はあったが、後の限定再試行で必要な16枚と本人確認待ちの修正画像を取得した。

## 残る制約・人間のレビュー対象

1. 50場面すべての最終画像批評はしていない。コピー文字516件の全更新後も全画面の再exportは未実施。今回の確認範囲・getter・模擬検査を最終合格やアクセシビリティ証明と取り違えない。
2. 色token・Typography assetのSDK経由の変更は、既存図形への自動同期を保証しない。Apply／Update・行間・glyphの実反映を人間の編集手順で確認する。font-family tokenへの直接bindingは使わない。
3. 複雑な計測位置は固定配置を保持。Auto Layout化、hover／focus、radial ambient／shadowの仕上げ、幅の中間値での挙動は未完了。本番レスポンシブ実装を保証するものではない。
4. プロトタイプの全クリック・keyboard／screen-reader検証、API認可・本人確認・Cookie保存・session失効・実報酬・削除完了は、このデザイン検査では証明していない。管理者ログは取得API未提供のまま。

検査：`preview/check.mjs`→`penpot/r2-check.mjs`。後者は限定模擬で、Native互換性や描画の証明ではない。生成物は`/tmp/agd-r2/`、`/tmp/attos-ui-review/`のみでコミットしない。人間の選定／最終編集後にのみ本番RSXへ進む。

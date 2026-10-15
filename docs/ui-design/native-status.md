# R2 / Native作業状況

最終記録：2026-10-15。**制作途中。人間の最終承認・編集は未実施。**

## 実際に保存できたもの

既存ファイル`129977ce-683b-806a-8008-c9d0bedd3212`内に、旧R1と空のPage 1を残して追加した。

- `AGD R2 Draft / Components`：共通操作部品20種類。色・文字サイズ・角丸・paddingのtokens、Noto Sans JP／OrbitronのTypography assets。
- `AGD R2 Draft / Desktop`：12場面。
- `AGD R2 Draft / Mobile`：12場面。

各viewportで作成済みのキー：`login`、`dashboard`、`setup`、`settings`、`admin`、`admin-deleted`、`admin-deletion`、`admin-recovery`、`dashboard-new-uids`、`dashboard-on`、`dashboard-starrail`、`deleted`。

NativeのBoard／Text／小さなベクトルと共通部品コピーであり、PNGを画面として貼ったものではない。本番Dioxus・API・DB・MCP接続設定は変更していない。

## 検証の到達点

- 通常の`penpot-native`経由で接続・対象ファイル・既存R1の75ボードを確認。別SDK接続や接続設定の再修復は使っていない。
- Native Gradientのsetter/getter、2フォントの必要weight、10主要場面のSVG exportとローカルPNG描画を確認。途中のSVG変換方式はGradientを失ったため中止し、APIのGradientへ変更した。
- 最初のAI試作Login 1枚だけは、不正な既定strokeと日本語の不要な折返しを確認後、同一作業内で撤去・再作成した。R1や人間の編集済みボードは再生成していない。
- 境界幅0の要素へstrokeColor tokenを適用すると既定strokeができるため、実境界がある場合だけ適用。文字間隔と段落の利用可能幅も計測するよう修正し、模擬検査へ追加した。
- 独立担当が主要5画面×2viewportのNative PNGを確認。古い部品ラベル・明確な日本語欠けはなく、閉じたメニューと重ね順を阻害要因として報告した。最終合格ではない。
- `r2-finish.js`を10主要場面に実行し、閉じたメニュー6件を非表示化、片側境界2件を修正、代表導線40件・8つの開始Flowを作成。**この時点のファイル検証エラーは0**。その後に追加した14場面の最終検証ではない。
- HTML計測側もclosed details除外・header前面化・片側の罫線を修正。ローカルでは50場面＋入力値秘匿、重ね順・表示判定・初期設定ゲートの検査に成功。Nativeのメニュー修正後の確認PNGはまだ取得していない。

## 停止理由と残作業

次の`empty-1440`を送った時点で、MCPが「Penpot plugin tab suspended / heartbeatなし40秒」として拒否。残り26場面の取り込みは未実行で、無条件に再送していない。現在のJS storageとアクティブページが復帰後も残るかは未確認。

1. ユーザーがPenpotタブを前面にして再開したら、現在のファイル・ボードキー・`storage.agdR2.inProgress`を読み取り確認する。中断マーカーや人間の変更を無視してガードを解除しない。
2. 生成済み50 JSONと保存済みキーを照合し、不足分だけ1場面ずつ追加。`reference-style.json`やprivacy検査JSONは画面ではないため送らない。1行50KBを超えるJSONをreadで切り捨てたまま実行しない。
3. 全体へ`r2-finish.js`を適用し、初期設定がSettings／Adminへ移動できないこと、disabled操作・未提供ログにリンクがないことを確認。Native導線は代表的な固定場面であり、フォーム入力、ページ内scroll、保存通知、全スイッチ組合せのシミュレーションではない。
4. Design System説明ボード、複雑なAuto Layout、hover／focus・shadowなどのNative仕上げ、共通部品・書体・色の実共有変更と復元を検査する。font-family tokenの直接bindingは保証せず、Typography assetsで管理する。
5. メニュー・Mobile・管理者ダイアログ・本人確認待ち・queued／unknown・削除受付をまとめてexportし、独立レビュー後の確認を行う。新規追加14場面はまだ画像レビューしていない。完了・採用承認と取り違えない。

再実行には`preview/check.mjs`→`penpot/r2-check.mjs`を使用。後者は限定的な模擬検査で、実NativeのAPI互換性や描画の証明ではない。生成画像／JSONは`/tmp/agd-r2/`と`/tmp/attos-ui-review/`にあり、コミットしない。

# R1 — 旧AttOsGameDashboard UIデザイン案

**履歴として保存。最新の指示は[revision-2.md](revision-2.md)、現在の入口は[README.md](README.md)。旧案は未承認・最終検証未完了であり、本番Dioxusには反映していない。**

## Penpotを開く

接続先ファイル: **New File 1** / `129977ce-683b-806a-8008-c9d0bedd3212`。

- [比較案 A / B](https://design.penpot.app/#/workspace?file-id=129977ce-683b-806a-8008-c9d0bedd3212&page-id=609d2656-00af-8046-8008-ca15c8b814d2) — `Draft / 01 / Concepts`
- [Desktopの編集](https://design.penpot.app/#/workspace?file-id=129977ce-683b-806a-8008-c9d0bedd3212&page-id=609d2656-00af-8046-8008-ca15c8b8f352) — `Draft / 02 / Desktop`
- [Mobileの編集](https://design.penpot.app/#/workspace?file-id=129977ce-683b-806a-8008-c9d0bedd3212&page-id=609d2656-00af-8046-8008-ca15c8b9979b) — `Draft / 03 / Mobile`
- [部品マスター](https://design.penpot.app/#/workspace?file-id=129977ce-683b-806a-8008-c9d0bedd3212&page-id=609d2656-00af-8046-8008-ca15c8ba4085) — `Draft / 04 / Components`
- 共通設定ページ: `Draft / 00 / Design System`。ボードは保存済み。**全体の最終描画検証は未完了**。

リンクは接続先のPenpotにログインして使う。URLで開けない場合はファイルIDとページ名で探す。開始時の空の `Page 1` は残し、承認済みデザインは上書きしていない。

## コンセプト比較

| | A / Orbit Folio（推奨候補） | B / Arcade Index（比較案） |
|---|---|---|
| 見た目 | コバルトのナビ・ミント／ラベンダーのゲーム面 | プラムの背景・アプリコットの選択色 |
| 構成 | 選択ゲームを主役に、UID・ボーナスを下に配置 | UID・ゲーム・ボーナスの3カラム |
| 長所 | 日本語が読みやすく、1カラムへ再構成しやすい | Desktopでボーナスが早く目に入る |
| 課題 | Mobileは情報量が多く縦長になる | 狭いゲーム欄・長いラベルへの余裕が少ない |

Aを推奨し、Bの明確な派遣見出しとボーナスへの導線を取り入れる。画面構造は通常のWeb操作のまま、ゲームごとの面色・current/max/full-atのまとまりで個性を出す。軌道模様や公式キャラクター画像、グリッチ等は追加しない。最終採用は人間が判断する。

## 共通設定とレスポンシブ

数値は [`tokens.json`](tokens.json)。Penpotの `Tokens > attos-orbit` に配色・余白・角丸・文字サイズを登録し、要素に適用する。本文はNoto Sans JP、ブランドと大きな数値はBarlow。本文16px、補足14px、見出し20／32px。Typography assetsの共通書体を編集できる。

Desktopは1440px、左ナビ232px、内側余白56px。Mobileは390px、余白20px、1カラム、48pxの操作部品。フォーム・ユーザー一覧は横スクロールの縮小版にせず、上下のパネルへ再構成する。後続CSSは内容に応じたbreakpoint、Flex/Grid、可変高さを使用する。390pxのアートボードは実装の固定幅指定ではない。

font-familyトークンの直接適用は接続プラグインでエラーになったため、**書体の共通変更はTypography assets**で行う。家族名トークンは登録情報として残し、適用済みと偽らない。サイズはトークン、色・余白・角丸も可能な箇所でトークンに結び付ける。

## 画面とDioxus対応案

新しいAPIや公開routeは追加しない。画面名は `/` 内の既存Page／Phaseを視覚的に整理したもの。詳細・状態ボードも既存レスポンスの表示例であり、新規endpointを意味しない。

| Penpot画面 | 対応する既存責務 | 主な部品 |
|---|---|---|
| ログイン・読み込み・パスキー待機 | `pages/auth.rs`, `app/session.rs`, `webauthn.rs` | SignInForm、Status、Button、Field |
| 初期設定・HoYoLAB本人確認 | `pages/auth.rs`, `pages/hoyolab.rs` | SetupStatus、PasswordCard、CookieCard |
| ダッシュボード・スターレイル・取得エラー・Empty | `pages/dashboard.rs`, `app/data.rs` | GameStatusCard、Metric、ExpeditionList、UID selector |
| UID表示設定 | `pages/accounts.rs` | AccountTable／MobileのAccountRow |
| ログインボーナス・202受付・実行詳細 | `pages/rewards.rs` | RewardSettingCard、RewardHistory、RunState |
| セキュリティ | `pages/security.rs` | PasswordCard、PasskeyCard、Field |
| ユーザー管理・有効／無効ユーザー操作 | `pages/admin.rs` | AdminCard、AdminUserRow |
| アカウント削除・受付 | `pages/deletion.rs` | DeleteAccountCard、Danger notice |
| 読み取り専用API仕様 | `pages/docs.rs` | OpenAPI viewer（try-itなし） |
| ナビ・Mobileメニュー・権限不足 | `pages/shell.rs`, `app/state.rs` | Header、NavButton、Status |

再利用の基礎は既存Dioxus Components由来のButton/Cardと既存Field。PenpotのButton／Input／Checkbox／Notice等をこれらへ対応させる。最終RSX、CSS、状態管理の変更は次の指示で行う。

## 調査結果と仕様差分

正本は `docs/spec/AttOsGameDashboard.md` と確定済み計画、補足は `docs/design/AttOsGameDashboard.md`、HTTPフィールドは `docs/dashboard-api.json`。開始時HEADは `fd72268`。最初の作業ツリーには未追跡のSkillsと `.pi/` があり、これらは成果物としてコミットしない。assetsは既存CSSのみ。

- HoYoLAB連携・手動／定期報酬は通常機能。暗号化設定、PW変更、検証済み連携など現在の条件に従う。利用承認は上流契約の実証ではない。ユーザーの追加指示によりdocsの古い制限説明を削除・更新した。API構造は変更していない。
- 現行UIの本人確認／報酬ボタンは、利用可否の説明があっても一部の無効化条件が不足する。デザインはpending・busy・実際の設定不足を区別する。Cookie保存を本人確認成功、202受付を受取成功として扱わない。
- ページ単位のLoadingと真のEmptyが不十分。UID・履歴・パスキーの未取得を空として断定しない。Linkの通常画面で取得エラーの表示が不足する。
- 現行画面に出ていない検出日時、must_change_password、履歴の試行番号／開始・終了／安全なコードは既存DTO内の情報として詳細表示へ整理できる。
- セッション固定期限、復旧時の失効、再有効化時の自動OFF、削除受付と背景完了の区別を説明する。削除完了の進捗APIは追加しない。
- 原神・スターレイルの上限や回復推定値は製品定数にしない。例の142/200、178/300などは合成レスポンス値。欠落値は未取得と表示する。

## Skillsの利用実績と検証

[`review.md`](review.md)に実行した機能、比較批評、修正、検証範囲を記録。CLIが動かなかったものや未検証項目を利用済みとは数えない。

制作スクリプトは `penpot/` に保存。Penpot Plugin API用で、製品コード・ブラウザ配信用コードではない。**編集後のPenpotが正であり、再実行して人間の修正を上書きしない**。`setup.js` は空の作業用ページの初回制作のみ。接続し直す場合は `bind.js` と関数定義を読み込み、必要な操作を小さい単位で実行する。長い一括作成はMCPタイムアウトの可能性がある。

## 人間の判断・未完了

- A／Bどちらの方向性を採用するか、彩度、Barlowの数値、日本語ラベル、Mobileの情報順序と密度を判断する。
- Penpotで本文・ボタン・背景・カードを個別に編集し、共通部品はマスター／Assetsで変更する。画像一枚で作った画面ではない。
- 保存済みの75ボード・26部品を内蔵MCPで確認したが、コピー文字の描画キャッシュとMobile見出しの問題が残った。全体の修正処理はRPCエラーで中断し、最終状態は未検証。完成を宣言しない。最新指示ではこの旧方向性を再設計する。
- ネイティブSVGをChromiumで描画して視覚検査できたが、これは製品のHTML／RSXや認証器の動作検証ではない。実機、Safari、200%拡大、キーボード／スクリーンリーダーは次工程。
- 実Cookie・実アカウント・本番DB・上流APIを使用していない。React等への移行も、本番UI/API/DBの変更もしていない。

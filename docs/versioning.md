# アプリとwebのバージョン分離（設計メモ）

状態: **設計案**（t_ec22f080）。実装は後続タスク t_98babe64（バージョン定義・タグ・CI）と t_5f2f91ce（アプリの更新判定・バージョン表示）、検証と手順の最終化は t_15b4aabc が行う。実装が済んだら、このメモの「現状」節を履歴に縮め、手順は [releases.md](releases.md) に移す。

このメモでいう2つの配布物は次のとおり。

| 呼び名 | 中身 | 配布の仕方 |
| --- | --- | --- |
| **web** | サーバー（`src/`、axum + DB）と、そのサーバーが配るブラウザ版UI（`dashboard-ui` の `web` feature を `dx build --platform web` したwasm） | コンテナイメージ `git.attodao.cc/attodao/attoshoyodashboard:<version>`。attos-nix-modules の `image =` 行を更新して attobox に反映 |
| **アプリ** | Androidアプリ（`dashboard-ui` の `mobile` feature を `build-android --release`） | Forgejoリリースに添付するAPK + `SHA256SUMS`。インストール済みアプリは自動更新で取得 |

サーバーとブラウザ版UIは同じイメージに入り、必ず一緒にデプロイされるので、1つの「web」バージョンで扱う（分けない）。

## 1. 現状（v1.8.0 時点の調査結果）

### 1.1 バージョンの定義場所

| 場所 | 値 | 使われ方 |
| --- | --- | --- |
| `Cargo.toml` `[package] version`（`attoshoyodashboard`） | 1.8.0 | サーバーのクレート。コード中では参照していない（`CARGO_PKG_VERSION` の利用は `src/` に無い）。`scripts/release-version.sh` がタグとの一致を確認 |
| `dashboard-ui/Cargo.toml` `[package] version`（`attos-dashboard-ui`） | 1.8.0 | web UI とアプリの**両方**が使う共有クレート。下の4か所が `CARGO_PKG_VERSION` を読む |
| `Cargo.lock` | 上2つの写し | `cargo update --workspace --offline` で追従 |
| `docs/dashboard-api.json` `info.version` | 0.1.0 | OpenAPI文書の版。リリース番号とは無関係（今回も触らない） |
| `CHANGELOG.md` | `## [1.8.0]` | web とアプリの変更を1項目にまとめて記載 |

`dashboard-ui` の `CARGO_PKG_VERSION` を読む箇所:

- `dashboard-ui/src/pages/about.rs` の `VERSION` — Aboutページの表示。web とアプリで同じ値（ファイル先頭コメントも「web and app share the same values」）。
- `dashboard-ui/src/mobile/updates.rs` の `Version::current()` — 自動更新の比較対象。
- `dashboard-ui/src/mobile/http.rs` の `USER_AGENT` — `AHD-Android/<version> (Android)`。サーバーへのAPI呼び出しとForgejoへの更新確認で送る。
- dx が `dashboard-ui` のクレートバージョンをAPKの `versionName` に書き、`scripts/build-android.sh` がそこから `versionCode = X*10000+Y*100+Z` を計算する。`MainActivity.kt` の更新完了通知（`afterUpdate`）はこの `versionName` と比べる。

環境変数でバージョンを渡す仕組みは無い（`AHD_SERVER_URL`・`AHD_UPDATE_*` は接続先だけ）。

### 1.2 タグとリリース

- タグは `vX.Y.Z`（正式）と `vX.Y.Z-<suffix>`（テスト、例 `v1.8.0-test.1`）。changesets・semantic-release 等は使っていない。人が `Release X.Y.Z` コミット（両Cargo.toml + Cargo.lock + CHANGELOG）をmainにマージし、マージコミットに注釈付きタグを打つ（[releases.md](releases.md)「リリースの手順」）。
- `scripts/release-version.sh <tag>` が `X.Y.Z` を**両方の**Cargo.tomlと照合し、`VERSION`・`APP_VERSION`・`VERSION_CODE`・`PRERELEASE` を出す。テストは `scripts/release-version.test.sh`（`devenv shell -- test-release`、`devenv test` に含まれる）。
- Forgejoリリースは `android.yml` が `scripts/publish-release.sh` で作る（テストタグはプレリリース）。v1.0.0〜v1.3.0 はタグだけでリリースが無い。v1.4.0 以降は毎回リリース + APK がある。

### 1.3 CI/CD のトリガー

| ワークフロー | トリガー | 出力 |
| --- | --- | --- |
| `.forgejo/workflows/image.yml` | `push: tags: ['v*']`（パス指定なし） | イメージ `X.Y.Z`（正式タグのときは `X.Y`・`latest` も） |
| `.forgejo/workflows/android.yml` | `push: tags: ['v*']`（パス指定なし） | タグのForgejoリリース、`ahd-android-X.Y.Z-arm64.apk`、`SHA256SUMS` |

mainへのpushやPRでは何も動かない（PR CIなし、テストは人が `devenv test`）。webの本番反映はイメージ公開後に attos-nix-modules の `modules/attos-hoyo-dashboard/nixos.nix` の `image =` 行（tag + digest）を更新するPR → server-nix の flake.lock 更新 → attobox で `nixos-rebuild switch`。

### 1.4 自動更新の判定

`dashboard-ui/src/mobile/updates.rs`（起動時に1回、`app/updates.rs::use_update_check` から）:

1. `GET https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard/releases/latest` を1回呼ぶ（`AHD_UPDATE_FORGEJO_URL`・`AHD_UPDATE_REPOSITORY` でビルド時に変更可）。
2. `evaluate`: draft/prerelease は `NotStable`。`tag_name` を `Version::parse`（先頭の `v`/`V` だけを許す）し、読めなければ `BadTag`。SemVerプレリリースも `NotStable`。
3. `ahd-android-<latest>-<arch>.apk` のアセットがあり、かつ `latest > Version::current()` のときだけ `update_available`。

Forgejo の `releases/latest` は `is_draft=false AND is_prerelease=false AND is_tag=false` の中で作成日時が最新のものを返す（Forgejo `models/repo/release.go` の `GetLatestReleaseByRepoID`。2026-11-06 に確認。現在の値は `v1.8.0`）。**リリースを作らずタグだけをpushした場合は対象にならない**（v1.0.0〜v1.3.0 がその例で、一覧にも latest にも出ない）。

### 1.5 結合している理由（問題の正体）

更新判定ロジック自体はアプリのバージョン（`dashboard-ui` のクレートバージョン）とアプリのリリースだけを見ている。結合は次の2点にある。

1. **1つの `v*` タグが両方のワークフローを起動する**。webだけ直して `vX.Y.Z` を打つとAPKとリリースも作られ、`releases/latest` が進み、既存アプリが「更新あり」を出す。アプリだけ直して打つとイメージも作られ `latest` が動く。
2. **`release-version.sh` が両方のCargo.tomlに同じ値を要求する**ので、片方だけ上げることができない。`dashboard-ui` のバージョンはweb UIのAboutにも出る。

### 1.6 アプリとwebの依存関係

- アプリはwebのコンテンツをリモート読み込みしない。UIはAPKに組み込まれ（`dashboard-ui` を `mobile` feature でビルド）、サーバーとはJSON APIでだけ話す。したがって**webだけの変更（ブラウザ版UIの見た目など）はアプリには届かない**。アプリに出したい共有UIの変更はアプリのリリースが必要。
- アプリが依存するwebの部分:
  - `/api/v1/...` のJSON API（`dashboard-ui/src/api.rs` 経由。`Origin` はビルド時の `AHD_SERVER_URL`）。
  - ブラウザで開くwebページ: `navigation::open_web_version`（現在のページのパス = `page_path`）、ブラウザログインの受け渡し `/app-login?challenge=…`（`mobile/browser_login.rs`）と `POST /api/v1/me/app-login`・`POST /api/v1/auth/app-login/exchange`、戻り先 `cc.attodao.ahd://login`。
  - 受信箱・通知設定（`app_events`）などアプリ専用のAPIフィールド。
- 共有コード: `dashboard-ui` クレート全体（`api.rs`・`models.rs`・`i18n.rs`・`locales/`・`pages/`・`components/`・`assets/` など）。アプリ専用は `src/mobile/`・`#[cfg(feature = "mobile")]` のモジュール（`app/{device_notifications,home_widgets,updates}.rs`、`pages/{app_update,device_notifications}.rs`）・`android/`・`assets/mobile.css`。名前に反して `pages/app_login.rs` はブラウザ側の承認ページ（web）で、`pages/app_nav.rs` はモジュール自体は共通（中身の一部が `cfg`）。web専用は `src/`（サーバー）・`Dockerfile`。共有クレート内でも `platform::APP` / `cfg` で片側だけに効く変更がある。

## 2. 決定事項

### 2.1 バージョンの持ち場所

| 配布物 | バージョンの正本 | 備考 |
| --- | --- | --- |
| web | ルート `Cargo.toml` の `[package] version`（`attoshoyodashboard`） | イメージタグ・OCIラベル・web版Aboutの表示 |
| アプリ | `dashboard-ui/Cargo.toml` の `[package] version`（`attos-dashboard-ui`） | dx がAPKの `versionName` に使うため、ここ以外に置けない。`Version::current()`・User-Agent・アプリ版Aboutの表示 |

- 2つの値は独立して上げる。等しくある必要はない（現在はどちらも 1.8.0 から始まる）。
- `dashboard-ui` はweb UIのビルドにも使われるが、**そのクレートバージョンはアプリのバージョンとして扱う**。web版UIが自分のバージョンを表示するときはルート `Cargo.toml` の値を使う（2.6）。
- 新しいバージョンファイルや環境変数は作らない（`dashboard-ui` のクレートバージョン以外にアプリの版を置く場所は dx の都合で無く、webはサーバークレートの版で足りる）。

### 2.2 タグ命名規則

| 配布物 | 正式 | テスト用（プレリリース） | 起動するワークフロー |
| --- | --- | --- | --- |
| アプリ | `vX.Y.Z` | `vX.Y.Z-<suffix>` | `android.yml` だけ |
| web | `web-vX.Y.Z` | `web-vX.Y.Z-<suffix>` | `image.yml` だけ |

- **アプリは `v*` をそのまま使い、`app-v*` にはしない**。理由: インストール済みのv1.8.0（自動更新を持つ最初の版）は `releases/latest` の `tag_name` を「先頭 `v` + SemVer」としか読めない。アプリのタグを `app-v1.9.0` にすると v1.8.0 は `BadTag` になり、以後一度も更新を検知できない。橋渡し版を挟んでも、橋渡し版を開かなかった端末は取り残される。`v*` を保てば既存端末の更新経路は何も変わらない。
- **webのタグではForgejoリリースを作らない**（タグだけ）。`releases/latest` は `is_tag` の行を返さないので、web のタグをいくつ打ってもアプリの更新判定は動かない。webのリリースノートは `CHANGELOG.md` に書く。
- `X.Y.Z` 部分のルール（数字の先頭0禁止、suffixは英数字を `.`/`-` でつないだもの）は両方共通。`Y`・`Z` < 100 はアプリだけの制約（versionCode）だが、単純さのため両方に課す。
- タグは注釈付きで、`Release app X.Y.Z` / `Release web X.Y.Z` のメッセージ。
- 既存のタグ（`v1.0.0`〜`v1.8.0` とテストタグ）は書き換えない。「`v1.8.0` 以前の `v*` は web とアプリの両方を出した」と [releases.md](releases.md) に注記する。
- `web-v1.8.0` は作らない（作ると新しい `image.yml` が 1.8.0 を再ビルドし、別digestで `1.8.0`・`1.8`・`latest` を上書きする）。webの最初の分離リリースは `web-v1.8.1` 以降。CHANGELOG の比較リンクは最初だけ `v1.8.0...web-v1.8.1` とする。
- イメージのタグは今までどおり `X.Y.Z`・`X.Y`・`latest`（`web-` も `v` も付けない）。レジストリと attos-nix-modules の参照形式は変わらない。

### 2.3 CI のトリガー

- `android.yml`: `on.push.tags: ['v*']` のまま（Forgejo のタグフィルタはglobで、`v*` は `web-v…` に一致しない）。`release-version.sh` の照合先を `dashboard-ui/Cargo.toml` だけにする。
- `image.yml`: `on.push.tags: ['web-v*']` に変える。最初のステップで `release-version.sh` を実行し、ルート `Cargo.toml` と照合してからビルドする。`version="${TAG#web-v}"`、OCIラベル `org.opencontainers.image.version` も同じ値。
- パス指定のトリガー（mainへのpushでの自動デプロイ）は**採用しない**。今もデプロイはタグ起点 + attos-nix-modules のPRで、Forgejo Actions の `paths` フィルタはタグpushには効かない。どちらを出すかは2.4のルールと `scripts/release-scope.sh`（3.1）で人が決める。

### 2.4 共有コードを変えたときにどちらを上げるか

基準は「**その配布物の利用者から見える動作が変わるか**」。テスト・文書・devenvだけの変更ではどちらも上げない。

| 変更したパス | 上げるもの |
| --- | --- |
| `src/**`、`Dockerfile`、`docs/dashboard-api.json`、ルート `Cargo.toml`（サーバーの依存）、`.forgejo/workflows/image.yml` | web |
| `dashboard-ui/android/**`、`dashboard-ui/src/mobile/**`、`dashboard-ui/assets/mobile.css`、`#[cfg(feature = "mobile")]` だけのモジュール（`app/{device_notifications,home_widgets,updates}.rs`、`pages/{app_update,device_notifications}.rs`）、`scripts/build-android.sh`、`scripts/publish-release.sh`、`.forgejo/workflows/android.yml`、`dashboard-ui/android/release-signing-cert.sha256` | アプリ |
| それ以外の `dashboard-ui/**`（`src/`・`assets/`・`locales/`・`Dioxus.toml`・`Cargo.toml` の依存）、`Cargo.lock` の依存更新、`rust-toolchain.toml` | **差分を読んで判断**: `platform::APP` / `!APP` / `cfg` で片側にしか効かないなら片側だけ、両方に効くなら両方 |
| `tests/**`、`dashboard-ui/tests/**`、`docs/**`（`dashboard-api.json` 以外）、`README.md`・`CHANGELOG.md` などルートの文書、`devenv.*`、`nix/**`、`.agents/**`・`.pi/**`・`.impeccable/**`、`scripts/*.test.*`、開発・リリース用の補助スクリプト（`release-version.sh`・`release-scope.sh`・`launch-*`・`test-android.sh`・`subset-fonts.sh`） | なし |
| 上のどれにも当たらないパス | `release-scope.sh` は `other` と表示する。表に足すか手で判断 |

- 両方に効く変更は両方を上げる。同じコミットに `vX.Y.Z` と `web-vA.B.C` を打ってよい（番号は別々）。
- 共有コードの変更でも、片方の配布物で意味が無いなら上げなくてよい（例: アプリで非表示の `if !APP` ブロックの修正はwebだけ）。迷ったら両方上げる（不要な更新は「使いづらい」程度、出し忘れは不具合）。
- Rust toolchain や依存（dioxus など）の更新は両方に効くのが普通だが、急がない側は次のリリースにまとめてよい。

### 2.5 互換性（アプリ ⇄ web API）

本番サーバーは1つ（`https://hoyo.attodao.cc`）で、アプリは自動更新を案内するだけで強制しない。よって**新しいwebと古いアプリが同時に動く**のが常態になる。

- **サポート範囲**: webは、自動更新を持つ最初の版 **アプリ 1.8.0 以降の全正式版** と互換を保つ（範囲を狭めるときは2.5.2の手順で）。
- **webの変更ルール**（API・ページとも）:
  - JSONの応答フィールドを削除・改名・型変更しない。アプリが必須として読むフィールド（`Option` でも `serde(default)` でもないもの）は特に。追加は自由。
  - リクエストの新しいフィールドはサーバー側で省略可能にする（古いアプリは送らない）。
  - アプリが開くwebのパス（`page_path` の各ページ、`/app-login?challenge=`）と `/api/v1/me/app-login`・`/api/v1/auth/app-login/exchange` の形を変えない。
  - エラーコード（`invalid_input` など）の意味を変えない。新しいコードを足すと古いアプリでは汎用メッセージになる。
- **アプリの変更ルール**: 新しいAPIが要るアプリの機能は、**先にwebをデプロイしてから**アプリのタグを打つ。CHANGELOG のアプリ項目に「web X.Y.Z 以降が必要」と書く。
- **最小対応アプリバージョンの強制は今は実装しない**（壊す変更が無い間は不要で、AGENTS.md の「先回りのインターフェースを作らない」に従う）。

#### 2.5.1 壊す変更がどうしても必要なとき（手順）

1. 新旧どちらの形も受け付けるアプリを出す（アプリのリリース）。
2. 利用者の更新を待つ（`sessions.user_agent` の `AHD-Android/<version>` で残っている版を確認できる）。
3. webで古い形をやめる。

#### 2.5.2 強制が必要になったときの形（将来）

アプリはすでに全APIリクエストに `User-Agent: AHD-Android/<version> (Android)` を付けているので、ヘッダーを足さずに実装できる。

- サーバー: `MIN_APP_VERSION` 定数を1か所（認証・HTTP境界の共通ミドルウェア）に置き、`AHD-Android/` で始まり版がそれ未満の要求に `426 Upgrade Required` + エラーコード `app_update_required` を返す。ブラウザ（UAが別）には影響しない。
- アプリ: `app_update_required` を受けたら更新ダイアログを出す。**この処理を持たない古いアプリには汎用エラーとしか見えない**ので、強制を始める前に、この処理を入れたアプリを一度リリースしておく。

## 3. 変更するファイルと方針

### 3.1 t_98babe64: バージョン定義・タグ・CI

| ファイル | 変更 |
| --- | --- |
| `scripts/release-version.sh` | タグの接頭辞で配布物を決める。`v…` → `COMPONENT=app`、`dashboard-ui/Cargo.toml` とだけ照合。`web-v…` → `COMPONENT=web`、ルート `Cargo.toml` とだけ照合。出力に `COMPONENT` を足す（app: `COMPONENT`・`VERSION`・`APP_VERSION`・`VERSION_CODE`・`PRERELEASE`、web: `COMPONENT`・`VERSION`・`PRERELEASE`）。`android.yml` は `COMPONENT=app` 以外なら失敗、`image.yml` は `web` 以外なら失敗させる（ステップ内で確認）。エラー文は「Tag must be vX.Y.Z or web-vX.Y.Z …」「Tag v1.9.0 must match dashboard-ui/Cargo.toml (1.8.0)」の形 |
| `scripts/release-version.test.sh` | 両方の配布物について accepts/rejects を足す。必須ケース: アプリとwebで別の版（`1.9.0`/`1.8.1`）が通る、`v` タグはルートの版を見ない、`web-v` タグは `dashboard-ui` の版を見ない、`web-v1.8.1-test.1` がプレリリース、`app-v1.9.0`・`webv1.8.1`・`web-1.8.1` は拒否 |
| `.forgejo/workflows/android.yml` | トリガーは `v*` のまま。「Verify version」で `COMPONENT=app` を確認。先頭コメントを「アプリのタグ（`vX.Y.Z`）でAPKを出す。webは `image.yml`（`web-v*`）」に |
| `.forgejo/workflows/image.yml` | `tags: ['web-v*']`。checkout の直後に `bash scripts/release-version.sh "$GITHUB_REF_NAME" >> "$GITHUB_ENV"` + `COMPONENT=web` の確認。`version="${TAG#web-v}"`、OCIラベルも同じ。正式版だけ `X.Y`・`latest` を動かす判定はそのまま |
| `scripts/release-scope.sh`（新規）+ `scripts/release-scope.test.sh` | `release-scope.sh [<app tag> [<web tag>]]`: 既定は最新の正式 `v*` / `web-v*` タグ（`web-v*` がまだ無ければ `v1.8.0`）から `HEAD` までの `git diff --name-only` を2.4の表で `web` / `app` / `shared` / `none` に分類して表示し、最後に「web: 上げる/不要」「app: 上げる/不要」「shared: 差分を読んで判断」を出す。判定はしない（表示だけ）。テストは一時リポジトリで各分類を確認。`devenv.nix` の `test-release` に追加 |
| `devenv.nix` | `scripts.test-release` に `bash scripts/release-scope.test.sh` を追加 |
| `docs/releases.md` | 冒頭の「`v*` タグで2つが動く」を、配布物ごとの表（2.2）に書き換える。「バージョンとタグ」「リリースの手順」を「アプリだけ」「webだけ」「両方」の3手順に分ける（下の3.4）。v1.8.0 以前の `v*` は両方を出していた注記。`web-v1.8.0` を作らない理由 |
| `README.md` | 「リリースイメージ」節: `web-v*` タグで公開、`web-v1.2.3` → `1.2.3`・`1.2`・`latest`。2段落目（同じタグでandroid.ymlも起動）を削除し、アプリは `v*` と releases.md へのリンクに。164行目付近のAndroid節の「`v*` タグをpushすると」は「アプリのタグ `vX.Y.Z`（`dashboard-ui/Cargo.toml` の版）をpushすると」に |
| `CHANGELOG.md` | 見出しを `## [アプリ 1.9.0] - 日付` / `## [web 1.8.1] - 日付` に分ける（1.8.0 以前の項目はそのまま）。冒頭の説明に「1.8.0 までは共通の版、以後はアプリとwebで別の版」。比較リンクは `[アプリ 1.9.0]: …/compare/v1.8.0...v1.9.0`、`[web 1.8.1]: …/compare/v1.8.0...web-v1.8.1`。アプリ項目には必要なweb版を書く（2.5） |
| `docs/android-changes.md` | 59行目付近の「`v*` タグのCI」を「アプリのタグ（`vX.Y.Z`）のCI」に、バージョンの正本が `dashboard-ui/Cargo.toml` である旨 |

このタスクではバージョン番号を上げない（次のリリースで上げる）。

### 3.2 t_5f2f91ce: アプリの更新判定とバージョン表示

1.5 のとおり、判定ロジックはすでにアプリの版とアプリのリリースだけを見ている。分離後も `releases/latest` を使い続ける（v1.8.0 端末と同じ経路に揃えるため。web はForgejoリリースを作らないので latest は常にアプリのリリース）。

| ファイル | 変更 |
| --- | --- |
| `dashboard-ui/src/mobile/updates.rs` | ロジックは変えない。モジュールコメントと `Version::current()` のdocを「アプリのバージョン（`dashboard-ui/Cargo.toml`、アプリのタグ `vX.Y.Z` と同じ）。webの版（ルート `Cargo.toml`・`web-v*`）とは無関係」に。テストを追加: (a) `tag_name: "web-v1.9.0"` のリリースは `BadTag` で `update_available` にならない（誤ってwebのリリースを作った場合の確認）、(b) web がいくら進んでもアプリ版と同じタグなら更新なし（`v1.8.0` vs current `1.8.0`）、(c) アプリ版より新しい `v` タグ + APK で更新あり（既存テストで足りていれば追加不要） |
| `dashboard-ui/src/pages/about.rs` | 表示する版を配布物ごとに分ける: アプリ（`cfg(feature = "mobile")`）は `env!("CARGO_PKG_VERSION")`、web は `include_str!("../../../Cargo.toml")` からルートの `[package] version` を読む純関数 `package_version(manifest: &str) -> Option<&str>`（`[package]` 表の `version = "…"` だけを見る。`[dependencies]` の `version` を拾わない）。const にできないので `fn version() -> &'static str` で返し、取れなければビルド時に気付けるよう単体テストで実ファイルが読めることを確認する。ファイル先頭コメントを更新。ラベルは今の「バージョン」のままでよい（どちらの版かは画面で自明） |
| `dashboard-ui/src/mobile/http.rs` | 変更なし（`USER_AGENT` はアプリの版のまま。2.5.2 の判定に使う） |
| `README.md` | 166行目付近の自動更新の説明に「比べるのはアプリの版（`dashboard-ui/Cargo.toml`）と、アプリのタグ `vX.Y.Z` のリリース。webのタグ `web-v*` はリリースを作らないので判定に関係しない」を追記 |
| `docs/android-changes.md` | 5.5節（282行目付近）に同じ説明。「web更新はアプリに届かない（UIはAPKに組み込み）」を1.6の内容で追記 |

最小対応アプリバージョンの確認は実装しない（2.5）。

#### 単体テスト以外の確認手順（PR本文に書くもの）

- web版Aboutの版: ルート `Cargo.toml` を一時的に 1.8.1 にして `devenv shell -- build-ui` → `test-ui` の About 確認、またはブラウザで `/about` を開いて 1.8.1 になること（元に戻す）。
- アプリ版Aboutの版: `build-android` のAPKを attodesk のエミュレーターで開き、About が `dashboard-ui/Cargo.toml` の版であること。
- 更新判定: 既存の updater E2E キット（attodesk `~/work/t5a`、Node のモック）で、`releases/latest` に `web-v1.9.0`（APKなし）を返すと「更新あり」が出ず logcat に `BadTag`、`v1.9.0` + APK で従来どおり更新ダイアログが出ること。

### 3.3 リポジトリ外（t_98babe64 の完了時に忘れずに）

- Hermes スキル `attos-hoyo-dashboard-development` の手順8〜9（`Release X.Y.Z` で両Cargo.tomlを上げる、`v*` タグでイメージが出る）と、`nix-container-image-updates` のAHDに関する記述を、`web-vX.Y.Z` でイメージ、`vX.Y.Z` でAPKに直す。
- attos-nix-modules・server-nix は変更不要（イメージのタグ形式 `X.Y.Z@sha256:…` は同じ）。

### 3.4 分離後のリリース手順（t_15b4aabc が releases.md に清書する骨子）

**webだけ**

1. ルート `Cargo.toml` の版を上げ、`cargo update --workspace --offline` で `Cargo.lock` を更新、`CHANGELOG.md` に `[web X.Y.Z]` を足した `Release web X.Y.Z` コミットをmainへ（`devenv test` を通す）。
2. `git tag -a web-vX.Y.Z -m "Release web X.Y.Z" <マージコミット> && git push origin refs/tags/web-vX.Y.Z`
3. `image` だけが動く。`android` が動かず、`releases/latest` が変わらないこと（`curl …/releases/latest | jq -r .tag_name`）を確認。
4. attos-nix-modules の `image =` 行を `X.Y.Z@sha256:<index digest>` に更新するPR → server-nix の flake.lock → attobox で switch。

**アプリだけ**

1. `dashboard-ui/Cargo.toml` の版を上げ、`Cargo.lock` を更新、`CHANGELOG.md` に `[アプリ X.Y.Z]`（必要なweb版があれば記載）を足した `Release app X.Y.Z` コミットをmainへ。
2. 新しいAPIに依存するなら、先にwebをリリース・デプロイしておく。
3. `git tag -a vX.Y.Z -m "Release app X.Y.Z" <マージコミット> && git push origin refs/tags/vX.Y.Z`
4. `android` だけが動く。リリースのアセットと `SHA256SUMS` を確認（今の手順3と同じ）。`image` が動かずレジストリの `latest` が変わらないこと。

**両方**: 1つの `Release` コミットで両方の版を上げ（コミット名 `Release app X.Y.Z, web A.B.C`）、同じマージコミットに `web-vA.B.C` と `vX.Y.Z` を打つ。webのデプロイを先に済ませてからアプリのタグを打つと、2.5 の順序を自然に守れる。

どれを出すかは `scripts/release-scope.sh` の出力と2.4の表で決める。

## 4. 検討して採らなかった案

- **`app-v*` / `web-v*` の対称なタグ**: v1.8.0 の更新判定が `app-v` を読めず、既存端末が更新を受け取れなくなる（2.2）。
- **webもForgejoリリースを作る**: `releases/latest` がwebのリリースを返し、v1.8.0 端末は `BadTag`（新しい版でも誤判定の危険）。webのリリースノートはCHANGELOGで足りる。
- **アプリの判定をリリース一覧の走査に変える**（`/releases?limit=N` から `v*` + APK の最新を探す）: 新しいアプリには有効だが、v1.8.0 端末は latest を見続けるので「webはリリースを作らない」制約は結局残る。現時点では利点が無いので見送り。webでリリースを作りたくなったときに再検討する。
- **バージョン専用ファイル・環境変数**: dx が `versionName` をクレートバージョンから作るため、アプリの版はクレートに置くほかない。二重管理を避ける。
- **パス指定による自動ビルド・デプロイ**: タグpushには `paths` が効かず、デプロイはもともとattos-nix-modules経由の手動。判断材料は `release-scope.sh` で出す。
- **今すぐ最小対応アプリバージョンを強制**: 壊す変更が無い間は不要（2.5.2 に将来の形だけ残す）。

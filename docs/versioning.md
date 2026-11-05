# アプリとwebのバージョン分離

状態: **実装済み**（PR #11。設計 t_ec22f080、実装 t_98babe64・t_5f2f91ce、統合検証 t_15b4aabc）。リリースの手順は [releases.md](releases.md) にあり、このメモは決定事項・その理由・検証記録を残す。

このメモでいう2つの配布物は次のとおり。

| 呼び名 | 中身 | 配布の仕方 |
| --- | --- | --- |
| **web** | サーバー（`src/`、axum + DB）と、そのサーバーが配るブラウザ版UI（`dashboard-ui` の `web` feature を `dx build --platform web` したwasm） | コンテナイメージ `git.attodao.cc/attodao/attoshoyodashboard:<version>`。attos-nix-modules の `image =` 行を更新して attobox に反映 |
| **アプリ** | Androidアプリ（`dashboard-ui` の `mobile` feature を `build-android --release`） | Forgejoリリースに添付するAPK + `SHA256SUMS`。インストール済みアプリは自動更新で取得 |

サーバーとブラウザ版UIは同じイメージに入り、必ず一緒にデプロイされるので、1つの「web」バージョンで扱う（分けない）。

## 早見表

| | アプリ | web |
| --- | --- | --- |
| バージョンの正本 | `dashboard-ui/Cargo.toml` の `[package] version` | ルート `Cargo.toml` の `[package] version` |
| 正式タグ / テスト用タグ | `vX.Y.Z` / `vX.Y.Z-<suffix>` | `web-vX.Y.Z` / `web-vX.Y.Z-<suffix>` |
| 起動するワークフロー | `.forgejo/workflows/android.yml` だけ | `.forgejo/workflows/image.yml` だけ |
| 出力 | Forgejoリリース + `ahd-android-X.Y.Z-arm64.apk` + `SHA256SUMS` | イメージ `X.Y.Z`（正式版は `X.Y`・`latest` も）。Forgejoリリースは作らない |
| リリースコミット | `Release app X.Y.Z` | `Release web X.Y.Z` |
| CHANGELOG の見出し | `## [アプリ X.Y.Z] - 日付` | `## [web X.Y.Z] - 日付` |
| 自動更新との関係 | `releases/latest` に出て、既存アプリが更新を検知する | `releases/latest` に出ないので、既存アプリは何も検知しない |

どちらを上げるかは 2.4 の表と `scripts/release-scope.sh` で決める。両方上げるときは同じマージコミットに2つのタグを打つ（番号は別々でよい）。

## 1. 分離前の仕組み（v1.8.0 まで）と結合の原因

- 2つの `[package] version`（ルート `attoshoyodashboard` と `dashboard-ui` の `attos-dashboard-ui`）は常に同じ値で、`Release X.Y.Z` コミットが両方を上げた。`scripts/release-version.sh` は `vX.Y.Z` を**両方の**Cargo.tomlと照合した。
- `image.yml` と `android.yml` はどちらも `push: tags: ['v*']`（パス指定なし）で動き、1つのタグでイメージとAPK・Forgejoリリースが両方出た。
- そのため web だけ直して `vX.Y.Z` を打つと `releases/latest` が進み既存アプリが「更新あり」を出し、アプリだけ直して打つとイメージの `latest` も動いた。

変わらなかった前提（分離後もそのまま使う）:

- `dashboard-ui` の `CARGO_PKG_VERSION` がアプリの版の唯一の置き場所: dx がAPKの `versionName` に使い、`build-android.sh` が `versionCode = X*10000+Y*100+Z` を計算し、`mobile/updates.rs` の `Version::current()`、`mobile/http.rs` の `USER_AGENT`（`AHD-Android/<version> (Android)`）、`MainActivity.kt` の更新完了通知がこれを使う。サーバーのクレート版はコード中で使っていない。`docs/dashboard-api.json` の `info.version`（0.1.0）はリリース番号と無関係。
- 自動更新（`mobile/updates.rs`、起動時に1回）は `GET …/releases/latest` を1回呼び、draft・prerelease・SemVerプレリリースを除き、`tag_name` を「先頭 `v` + SemVer」として読み（読めなければ `BadTag`）、`ahd-android-<latest>-<arch>.apk` があり `latest > Version::current()` のときだけ更新ありとする。判定ロジックはアプリの版とアプリのリリースしか見ていなかった。
- Forgejo の `releases/latest` は `is_draft=false AND is_prerelease=false AND is_tag=false` の中で最新を返す（Forgejo `models/repo/release.go` の `GetLatestReleaseByRepoID`）。**リリースを作らずタグだけをpushした場合は対象にならない**（v1.0.0〜v1.3.0 がその例。4 で `web-v1.9.0-test.1` でも確認）。

### 1.1 アプリとwebの依存関係

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

- 2つの値は独立して上げる。等しくある必要はない（どちらも 1.8.0 から始まる）。
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
- パス指定のトリガー（mainへのpushでの自動デプロイ）は**採用しない**。今もデプロイはタグ起点 + attos-nix-modules のPRで、Forgejo Actions の `paths` フィルタはタグpushには効かない。どちらを出すかは2.4のルールと `scripts/release-scope.sh`（3）で人が決める。

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

## 3. 実装（どのファイルが何をするか）

| ファイル | 役割 |
| --- | --- |
| `scripts/release-version.sh` | タグの接頭辞で配布物を決める。`v…` → `COMPONENT=app`、`dashboard-ui/Cargo.toml` とだけ照合し `VERSION`・`APP_VERSION`・`VERSION_CODE`・`PRERELEASE` を出す。`web-v…` → `COMPONENT=web`、ルート `Cargo.toml` とだけ照合し `VERSION`・`PRERELEASE` を出す。`app-v…`・`web-1.8.1` などは拒否 |
| `.forgejo/workflows/android.yml` | `tags: ['v*']`。「Verify version」で `COMPONENT=app` 以外なら失敗。APK・署名・`SHA256SUMS`・公開（`scripts/publish-release.sh`）は分離前と同じ |
| `.forgejo/workflows/image.yml` | `tags: ['web-v*']`。「Verify version」で `COMPONENT=web` 以外なら失敗。イメージタグは `VERSION`（`web-v` を除いた値）、正式版だけ `X.Y`・`latest` も動かす。Forgejoリリースは作らない |
| `scripts/release-scope.sh` | 前回の正式 `vX.Y.Z` / `web-vX.Y.Z`（`web-v` がまだ無ければ `v1.8.0`）から `HEAD` までの変更パスを 2.4 の表で `app`・`web`・`shared`・`none`・`other` に分類して表示する。判断はしない |
| `scripts/release-version.test.sh`・`scripts/release-scope.test.sh` | 上2つのテスト。`devenv shell -- test-release`（`devenv test` に含まれる） |
| `dashboard-ui/src/mobile/updates.rs` | 判定ロジックは v1.8.0 から**変更なし**（コメントとテストだけ追加）。`web-v…` のリリースが誤って作られても `BadTag` で更新にならないこと、新しい `v` タグなら更新になることをテストする |
| `dashboard-ui/src/pages/about.rs` | Aboutの版を配布物ごとに表示する。アプリは `CARGO_PKG_VERSION`（`dashboard-ui`）、web版はルート `Cargo.toml` を `include_str!` して `[package] version` を読む。`Cargo.lock` のリンク先も配布物のタグ（web は `web-vX.Y.Z`、ただし 1.8.0 以前は `vX.Y.Z`） |
| `docs/releases.md`・`README.md`・`CHANGELOG.md`・`docs/android-changes.md` | 手順・説明を配布物ごとに分けた |

リポジトリ外: attos-nix-modules・server-nix は変更不要（イメージのタグ形式 `X.Y.Z@sha256:…` は同じ）。Hermes スキル `attos-hoyo-dashboard-development` の手順8〜9は分離後の手順に更新済み。

## 4. 検証記録

実際の正式リリース（`vX.Y.Z`・`web-vX.Y.Z`）は、既存アプリに本当の更新を出しイメージの `latest` を動かすので打っていない。代わりに PR #11 の統合コミット（`1ed402d`）に、片方の版だけを 1.9.0 に上げた検証用コミット（mainには入れない）を作り、テスト用タグを本物のForgejoに push して確かめた。

| 検証用コミット | 変えた版 | タグ |
| --- | --- | --- |
| `38e8050`「TEST ONLY: web 1.9.0 …」 | ルート `Cargo.toml` だけ 1.9.0（アプリは 1.8.0 のまま） | `web-v1.9.0-test.1` |
| `e2b36af`「TEST ONLY: app 1.9.0 …」 | `dashboard-ui/Cargo.toml` だけ 1.9.0（webは 1.8.0 のまま） | `v1.9.0-test.1` |

事前の状態: `releases/latest` = `v1.8.0`（id 43、アセット `ahd-android-1.8.0-arm64.apk`・`SHA256SUMS`）、レジストリの `1.8.0`・`1.8`・`latest` = `sha256:ac4882b8…29ad2`、最後のActions実行は run 24。

| # | 検証内容 | 方法 | 結果 |
| --- | --- | --- | --- |
| 1 | webだけのリリースでwebだけが出て、アプリのリリースと更新判定が変わらない | `web-v1.9.0-test.1` をpushし、Actions（`/api/v1/repos/…/actions/tasks`）・リリース・レジストリを前後で比較。実機相当の確認として、本物の v1.8.0 リリースAPK（リリース鍵署名）を attodesk のエミュレーター（`ahd2`）に入れて起動し、本物のForgejoに対する更新確認のlogcatを見た | 動いたのは `image`（run 25、`head_sha` 38e8050、成功）だけで `android` は動かなかった。`releases/tags/web-v1.9.0-test.1` は404（リリース無し）、リリース一覧にも出ない。`releases/latest` は `v1.8.0` のまま。イメージは `1.9.0-test.1`（`sha256:a6498dd5…7597`、ラベル `org.opencontainers.image.version=1.9.0-test.1`）だけが増え、`1.8.0`・`1.8`・`latest` のdigestは不変、`1.9.0`・`1.9` は作られない。イメージ内のweb版wasmにはルート `Cargo.toml`（1.9.0）が埋め込まれている（web版Aboutの表示元）。v1.8.0 アプリのlogcat: `update check: current 1.8.0, latest 1.8.0, update available: false` |
| 2 | アプリだけのリリースでアプリのリリースが出て、webのデプロイが走らない | `v1.9.0-test.1` をpushし、同じく比較。APKを取得して `sha256sum -c SHA256SUMS`、`aapt2 dump badging`、`apksigner verify --print-certs` | 動いたのは `android`（run 26、`head_sha` e2b36af、成功）だけで `image` は動かず、レジストリに `1.9.0*` 以外の変化なし（`1.9.0-test.1` はwebのテストタグ由来）。リリース `v1.9.0-test.1` はプレリリースで、アセットは `ahd-android-1.9.0-test.1-arm64.apk`（20133414 B）と `SHA256SUMS` だけ、チェックサム一致。APKは `versionCode=10900`・`versionName=1.9.0`、署名者は1つでリリース鍵（`2e2a3f41…44ae`、v1.8.0 と同じ）。テスト用なので `releases/latest` は `v1.8.0` のまま（テストタグが既存アプリに出ないことも確認できた） |
| 3 | 共有コードの変更時に設計どおりの判断になる | `scripts/release-scope.sh` を統合コミットと、使い捨てのcloneで作った変更（共有 `pages/dashboard.rs`・アプリ専用 `mobile/updates.rs`・web専用 `src/main.rs`）に対して実行。`devenv shell -- test-release`（`release-version.test.sh`・`release-scope.test.sh`・`publish-release.test.mjs`） | 統合コミットでは `about.rs`（共有、両方の画面に効く）が `shared` と出て、app・web とも bump の判断材料が出る（2.4 のとおり両方上げる対象）。共有パスだけを変えた場合は app・web とも `decide` と差分コマンドが出て、自動では決めない（2.4 のとおり）。前回タグ `v1.9.0`・`web-v1.8.1` を置いた場合は各配布物が自分のタグから比較される。テストは全部通過 |
| 4 | 既存版のアプリから新しいアプリへの自動更新が途切れない | (a) v1.8.0 タグのソースに、上の実際のForgejo応答（`releases/latest`、`releases/tags/v1.9.0-test.1`、それを正式版 `v1.9.0` に読み替えたもの、誤って作った `web-v1.9.0` リリース）を食わせる使い捨てテストを足し、`cargo test -p attos-dashboard-ui -- updates` を実行。(b) エミュレーターで v1.8.0 リリースAPKの上に `v1.9.0-test.1` のAPKを `adb install -r` | (a) v1.8.0 の `evaluate` で、latest → 更新なし、テスト用リリース → `NotStable`、正式版に読み替えた同じ応答 → 更新あり（`ahd-android-1.9.0-arm64.apk`、チェックサムは `SHA256SUMS`）、web のリリース → `BadTag`。13件通過。(b) 上書きインストール成功（`firstInstallTime` 不変 = データ保持、`versionName=1.9.0`）、起動後の更新確認は `current 1.9.0, latest 1.8.0, update available: false`。更新判定・ダウンロード・検証・インストールのコード（`mobile/`・`app/`・`android/`・`build-android.sh`・`publish-release.sh`）は v1.8.0 から変わっていない（`updates.rs` はコメントとテストだけ） |

確認できていないこと:

- 正式な `vX.Y.Z` を本物の `releases/latest` に出したときの、v1.8.0 アプリの更新ダイアログからのダウンロードとインストールそのもの（本番の利用者に更新が出るため）。上の 4 と、v1.8.0 の更新機能のE2E（t_5a12c700、モックのForgejo）で代えた。最初のアプリの正式リリースの後、手元の端末で更新ダイアログが出て更新できることを確かめる。
- 正式な `web-vX.Y.Z` で `X.Y`・`latest` が動くこと（`PRERELEASE=false` の分岐は分離前と同じコード）。最初のwebの正式リリースで確かめる。
- 検証用のタグ `web-v1.9.0-test.1`・`v1.9.0-test.1`、プレリリース `v1.9.0-test.1`、イメージ `1.9.0-test.1` は残っている（削除にはトークンが要る）。検証用コミットはmainに入れない。

問題は見つからなかった。

## 5. 検討して採らなかった案

- **`app-v*` / `web-v*` の対称なタグ**: v1.8.0 の更新判定が `app-v` を読めず、既存端末が更新を受け取れなくなる（2.2）。
- **webもForgejoリリースを作る**: `releases/latest` がwebのリリースを返し、v1.8.0 端末は `BadTag`（新しい版でも誤判定の危険）。webのリリースノートはCHANGELOGで足りる。
- **アプリの判定をリリース一覧の走査に変える**（`/releases?limit=N` から `v*` + APK の最新を探す）: 新しいアプリには有効だが、v1.8.0 端末は latest を見続けるので「webはリリースを作らない」制約は結局残る。現時点では利点が無いので見送り。webでリリースを作りたくなったときに再検討する。
- **バージョン専用ファイル・環境変数**: dx が `versionName` をクレートバージョンから作るため、アプリの版はクレートに置くほかない。二重管理を避ける。
- **パス指定による自動ビルド・デプロイ**: タグpushには `paths` が効かず、デプロイはもともとattos-nix-modules経由の手動。判断材料は `release-scope.sh` で出す。
- **今すぐ最小対応アプリバージョンを強制**: 壊す変更が無い間は不要（2.5.2 に将来の形だけ残す）。

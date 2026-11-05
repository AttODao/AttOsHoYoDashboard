# リリースと配布物

アプリ（Android）とweb（サーバー＋ブラウザ版UIのコンテナイメージ）は、別々のバージョンと別々のタグでリリースします（設計と理由は [versioning.md](versioning.md)）。

| 配布物 | バージョンの正本 | 正式タグ | テスト用タグ | ワークフロー | 出力 |
| --- | --- | --- | --- | --- | --- |
| アプリ | `dashboard-ui/Cargo.toml` の `[package] version` | `vX.Y.Z` | `vX.Y.Z-<suffix>` | `.forgejo/workflows/android.yml` | そのタグのForgejoリリースと、そこに添付するアプリのパッケージ・`SHA256SUMS` |
| web | ルート `Cargo.toml` の `[package] version` | `web-vX.Y.Z` | `web-vX.Y.Z-<suffix>` | `.forgejo/workflows/image.yml` | linux/amd64のコンテナイメージ（Forgejoコンテナレジストリ）。README「リリースイメージ」を参照。Forgejoリリースは作らない |

- ワークフローのタグフィルタ（glob）は `v*` と `web-v*` で、互いに一致しません。アプリのタグではイメージが、webのタグではAPK・リリースが作られません。
- webのタグでForgejoリリースを作らないので、アプリの自動更新が見る `releases/latest` は常にアプリのリリースです。webのリリースノートは `CHANGELOG.md` に書きます。
- アプリのタグは `app-v…` ではなく `v…` のままです。インストール済みのv1.8.0は `releases/latest` のタグを「`v` + SemVer」としか読めないため、接頭辞を変えると更新を検知できなくなります。
- **v1.8.0 までの `v*` タグ**は、イメージとAPKの両方を出していました（2つの版は同じ値）。`web-v1.8.0` は作りません（作ると `image.yml` が1.8.0を別digestで作り直し、`1.8.0`・`1.8`・`latest` を上書きします）。webの最初の独立リリースは `web-v1.8.1` 以降です。
- 2つの版は等しくなくてかまいません。片方だけ上げます。

配布するアプリは現在Android（arm64）だけです。サーバーはコンテナイメージで配布し、リリースのアセットにはしません。

## バージョンとタグ

- `X.Y.Z` は、そのタグの配布物のCargo.tomlの `[package] version` とだけ一致しなければなりません（アプリのタグ → `dashboard-ui/Cargo.toml`、webのタグ → ルート `Cargo.toml`）。もう片方の版は見ません。
- テスト用のタグは `vX.Y.Z-<suffix>` / `web-vX.Y.Z-<suffix>`（例 `v1.8.0-test.1`、`web-v1.8.1-rc.1`。suffixは英数字を `.` か `-` でつなげたもの）です。アプリのテスト用タグのリリースは **プレリリース** として作られ、webのテスト用タグはそのタグのイメージだけを公開します。
- `Y` と `Z` は100未満です（アプリのversionCodeの制約。webにも同じ規則を課します）。APKの `versionName` は `X.Y.Z`、`versionCode` は `X*10000+Y*100+Z` です（`build-android` が設定します）。
- 判定は `scripts/release-version.sh <タグ>` が行い、どちらのワークフローも最初のステップでこれを実行します。出力の `COMPONENT`（`app` / `web`）が自分の配布物でなければ失敗し、条件を満たさないタグはビルドの前に失敗します。アプリはビルド後に `aapt2 dump badging` でAPK自身のパッケージ名・`versionCode`・`versionName` がタグと一致することも確認します。
  ```console
  $ bash scripts/release-version.sh v1.9.0
  COMPONENT=app
  VERSION=1.9.0
  APP_VERSION=1.9.0
  VERSION_CODE=10900
  PRERELEASE=false
  $ bash scripts/release-version.sh web-v1.8.1-test.1
  COMPONENT=web
  VERSION=1.8.1-test.1
  PRERELEASE=true
  ```

## どちらをリリースするか

基準は「その配布物の利用者から見える動作が変わるか」です（パスごとの表は [versioning.md 2.4](versioning.md#24-共有コードを変えたときにどちらを上げるか)）。`scripts/release-scope.sh` が、配布物ごとに前回のタグから `HEAD` までの変更パスを分類して表示します（判断はしません）。

```console
$ devenv shell -- bash scripts/release-scope.sh            # 前回の vX.Y.Z / web-vX.Y.Z（web-v が無ければ v1.8.0）から
$ devenv shell -- bash scripts/release-scope.sh v1.9.0 web-v1.8.1   # 比較元を指定
app: changes since v1.9.0
  app     dashboard-ui/src/mobile/updates.rs
  shared  dashboard-ui/src/pages/about.rs
  (4 path(s) outside the app ignored)
web: changes since web-v1.8.1
  web     src/dashboard/http/ui.rs
  shared  dashboard-ui/src/pages/about.rs
  (2 path(s) outside the web ignored)

app: bump (1 app path(s)); read the shared diff: git diff v1.9.0 HEAD -- dashboard-ui/src/pages/about.rs
web: bump (1 web path(s)); read the shared diff: git diff web-v1.8.1 HEAD -- dashboard-ui/src/pages/about.rs
```

- `bump`: その配布物だけのパスが変わったので上げる。
- `shared`: 共有UIクレート・`Cargo.lock`・toolchain。差分を読み、`platform::APP` / `cfg` で片側にしか効かないなら片側だけ、両方に効くなら両方上げる（迷ったら両方）。
- `classify by hand`: 規則にないパス。表に足すか、手で判断する。
- テスト・文書・開発環境だけの変更（`none`）はどちらも上げない。

## アセットの命名規則

```
ahd-{os}-{version}-{arch}.{ext}
SHA256SUMS
```

| 部分 | 値 |
| --- | --- |
| `{os}` | `android`（今後ほかのOS向けを追加するときは `linux`・`windows`・`macos` など小文字のOS名） |
| `{version}` | タグから `v` を除いたもの。正式リリースは `1.8.0`、テスト用タグは `1.8.0-test.1` |
| `{arch}` | `arm64`（今後は `x86_64` など） |
| `{ext}` | パッケージの形式。Androidは `apk` |

例: `v1.8.0` のリリースには `ahd-android-1.8.0-arm64.apk` と `SHA256SUMS` が付きます。この形式はv1.4.0以降の既存のAPK名と同じです。v1.7.0までは `SHA256SUMS` の代わりにAPKごとの `<APK名>.sha256` を付けていました。

アセットの取得に使うURLは、どちらもForgejoの標準のものです。

- 最新の正式リリース: `GET /api/v1/repos/AttODao/AttOsHoYoDashboard/releases/latest`（ドラフトとプレリリースは返りません）。`tag_name`・`body`（リリースノート）・`assets[].name`・`assets[].browser_download_url` を使います。
- ダウンロード: `https://git.attodao.cc/AttODao/AttOsHoYoDashboard/releases/download/<tag>/<asset名>`

アプリの自動更新は、自分の `{os}`・`{arch}` に合う名前のアセットを上の規則で選び、プレリリースは対象にしません。

## チェックサムと署名

アプリはインストール前に `SHA256SUMS`（またはAPKごとの `.sha256`）を検証し、APKの署名者がインストール済みアプリと一致することを確認します（[Android変更点 5.6](android-changes.md#56-更新の確認ダイアログとインストール)）。

- `SHA256SUMS` はそのリリースのパッケージ全部（`SHA256SUMS` 自身を除く）を `sha256sum` の形式（`<64桁の16進数><空白2つ><アセット名>`、1行1ファイル）で列挙します。`sha256sum -c SHA256SUMS` で検証できます。
- AndroidのAPKはリリース鍵（APK Signature Scheme v2/v3）で署名します。Androidは、インストール済みのアプリと同じ証明書で署名されたAPKしか更新としてインストールしません。そのためDioxusのアプリには独自の更新署名（Tauri updaterのような署名ファイル）は作らず、APK署名をそのまま更新の署名として使います。v4署名（`.idsig`、adbの差分インストール用）は作らないので、リリースのアセットはパッケージと `SHA256SUMS` だけです。ワークフローは `dist/` にそれ以外のファイルがあると失敗します。
- リリース鍵の証明書のSHA-256は `dashboard-ui/android/release-signing-cert.sha256` に置いています。ワークフローはAPKの署名者が1つだけで、その証明書がこの値と一致することを確認します。アプリ側はインストール済みアプリの署名証明書と比較します。各リリースの本文にも同じ値を書きます。
- 署名用のsecret（`ANDROID_KEYSTORE_BASE64`・`ANDROID_KEYSTORE_PASSWORD`・`ANDROID_KEY_ALIAS`）がない場合、正式リリースは失敗します。テスト用タグだけはビルドごとのデバッグ鍵で署名して続け、リリース本文にその旨を書きます（既存のアプリに上書きインストールできません）。
- 鍵を替えると既存のアプリは更新できなくなります。替えるときは証明書ファイルも同じコミットで更新してください。

## 公開の手順（ワークフローが行うこと）

1. アプリのタグ（`vX.Y.Z`）のリビジョンをcloneし、`scripts/release-version.sh` でバージョンを確認する（`COMPONENT=app` でなければ失敗）。
2. `devenv --profile android shell -- build-android --release` でAPKをビルドする。
3. リリース鍵で署名し、署名者の証明書・`versionName`・`versionCode` を確認し、`dist/` に `ahd-android-{version}-arm64.apk` と `SHA256SUMS` を作る。
4. `scripts/publish-release.sh dist` が、タグのリリースがなければ作り（テスト用タグはプレリリース）、`dist/` の全ファイルを添付する。同じ名前の古いアセットは置き換え、それ以外は残す。最後にリリースを読み直し、全アセットの名前とサイズを確認し、公開URLからダウンロードして `sha256sum -c SHA256SUMS` を通す。`SHA256SUMS` に載っていないファイルが `dist/` にあれば何もアップロードせずに失敗する。

## リリースの手順（人が行うこと）

### 共通の準備

1. どちらを出すか決める。`devenv shell -- bash scripts/release-scope.sh` を実行し、[どちらをリリースするか](#どちらをリリースするか)の基準で、アプリだけ・webだけ・両方のどれかを選ぶ。`shared` の行が出たら、その差分（出力に `git diff` のコマンドが出ます）を読み、`platform::APP` / `cfg` で片側にしか効かないか確かめる。迷ったら両方。
2. 新しい版を決める（SemVer。`Y`・`Z` は100未満）。2つの版は独立で、もう片方の版とそろえる必要はない。既に使われていないことを確かめる。
   ```bash
   git fetch --tags origin
   git tag --list 'v*' 'web-v*' --sort=-version:refname | head
   ```
3. Release コミットを作る。上げるCargo.tomlの `[package] version` だけを書き換え、`Cargo.lock` を追従させ、`CHANGELOG.md` に項目を足す。
   ```bash
   devenv shell -- cargo update --workspace --offline   # Cargo.lock のワークスペースのクレート版だけが変わる
   git diff --stat                                        # Cargo.toml（1つか2つ）・Cargo.lock・CHANGELOG.md だけのはず
   devenv test                                            # CIではテストを実行しないので、ここで通す
   ```
   CHANGELOG の見出しは `## [アプリ X.Y.Z] - 日付` / `## [web X.Y.Z] - 日付`、比較リンクは前回の同じ配布物のタグから（[CHANGELOG.md](../CHANGELOG.md) の冒頭）。webの最初の独立リリースだけ `v1.8.0...web-vX.Y.Z` です。
4. PRをmainにマージし、マージコミットのSHAを控える（以後のタグはそのSHAに打ちます。`origin/main` は後で動いていることがあります）。

### アプリだけ

1. 準備の3で `dashboard-ui/Cargo.toml` だけを `X.Y.Z` に上げる。コミット名は `Release app X.Y.Z`。新しいAPIが要るなら CHANGELOG の項目に「web A.B.C 以降が必要」と書き、**先にwebをリリース・デプロイしておく**（[versioning.md 2.5](versioning.md#25-互換性アプリ--web-api)）。
2. マージコミットに注釈付きタグを付けてpushする。
   ```bash
   git tag -a vX.Y.Z -m "Release app X.Y.Z" <マージコミット>
   git push origin refs/tags/vX.Y.Z
   ```
3. Actions（`https://git.attodao.cc/AttODao/AttOsHoYoDashboard/actions`）で `android` だけが動いて成功し、`image` が動かないことを確認する。リリースとアセットは次で確かめられます。
   ```bash
   API=https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard
   curl -sS "$API/actions/tasks?limit=5" | jq -r '.workflow_runs[] | "\(.run_number) \(.workflow_id) \(.head_branch) \(.status)"'
   curl -sS "$API/releases/tags/vX.Y.Z" | jq -r '.prerelease, .assets[].name'   # false、ahd-android-X.Y.Z-arm64.apk と SHA256SUMS
   curl -sS "$API/releases/latest" | jq -r .tag_name                            # vX.Y.Z（既存アプリが次の起動で更新を検知する）
   curl -sSLO https://git.attodao.cc/AttODao/AttOsHoYoDashboard/releases/download/vX.Y.Z/ahd-android-X.Y.Z-arm64.apk
   curl -sSLO https://git.attodao.cc/AttODao/AttOsHoYoDashboard/releases/download/vX.Y.Z/SHA256SUMS
   sha256sum -c SHA256SUMS
   ```
4. 失敗したジョブを再実行すると、同じ名前のアセットは置き換えられます。
5. webのデプロイ（attos-nix-modules）は不要です。レジストリの `latest` も変わりません。

### webだけ

1. 準備の3でルート `Cargo.toml` だけを `X.Y.Z` に上げる。コミット名は `Release web X.Y.Z`。`web-v1.8.0` は作らない（上の注意）ので、最初は 1.8.1 以上です。
2. マージコミットに注釈付きタグを付けてpushする。
   ```bash
   git tag -a web-vX.Y.Z -m "Release web X.Y.Z" <マージコミット>
   git push origin refs/tags/web-vX.Y.Z
   ```
3. Actionsで `image` だけが動いて成功し、`android` が動かないこと、Forgejoリリースが作られず `releases/latest` が変わらないこと（既存アプリは更新を検知しない）を確認する。
   ```bash
   API=https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard
   curl -sS "$API/actions/tasks?limit=5" | jq -r '.workflow_runs[] | "\(.run_number) \(.workflow_id) \(.head_branch) \(.status)"'
   curl -sS "$API/releases/latest" | jq -r .tag_name                    # アプリの最新タグ vA.B.C のまま
   curl -sS -o /dev/null -w '%{http_code}\n' "$API/releases/tags/web-vX.Y.Z"   # 404（リリースは無い）
   ```
4. イメージのindex digestを調べる。
   ```bash
   token=$(curl -sS 'https://git.attodao.cc/v2/token?service=container_registry&scope=repository:attodao/attoshoyodashboard:pull' | jq -r .token)
   curl -sSI -H "Authorization: Bearer $token" -H 'Accept: application/vnd.oci.image.index.v1+json' \
     https://git.attodao.cc/v2/attodao/attoshoyodashboard/manifests/X.Y.Z | grep -i docker-content-digest
   ```
5. attos-nix-modules の `modules/attos-hoyo-dashboard/nixos.nix` の `image =` 行を `X.Y.Z@sha256:<index digest>` に更新するPR → server-nix の `flake.lock` 更新 → attobox で `nixos-rebuild switch`。デプロイ後、ブラウザ版の About が `X.Y.Z` を表示します（アプリの About はアプリの版のまま）。

### 両方

1. 準備の3で両方のCargo.tomlを上げる（番号は別々でよい）。コミット名は `Release app X.Y.Z, web A.B.C`、CHANGELOG には `[web A.B.C]` と `[アプリ X.Y.Z]` の2項目を足す。
2. 同じマージコミットに、**webのタグを先に**打つ。
   ```bash
   git tag -a web-vA.B.C -m "Release web A.B.C" <マージコミット>
   git push origin refs/tags/web-vA.B.C
   ```
3. 「webだけ」の3〜5（イメージの確認とデプロイ）を済ませる。
4. 同じマージコミットにアプリのタグを打ち、「アプリだけ」の3〜4を行う。
   ```bash
   git tag -a vX.Y.Z -m "Release app X.Y.Z" <マージコミット>
   git push origin refs/tags/vX.Y.Z
   ```
   webのデプロイを済ませてからアプリのタグを打つと、互換性の順序（新しいAPIはwebが先）を守れます。アプリが新しいAPIに依存しないなら、2つのタグを続けて打ってもかまいません。

### 共有コードを変えたとき

`dashboard-ui` の共有部分（`pages/`・`components/`・`locales/`・`assets/` など）、`Cargo.lock` の依存更新、`rust-toolchain.toml` は、`release-scope.sh` が `shared` と表示し、自動では決まりません（規則は [versioning.md 2.4](versioning.md#24-共有コードを変えたときにどちらを上げるか)）。

- 両方の利用者に見える変更（共通の画面・文言・API呼び出しの修正など）→ **両方**。
- `if !crate::platform::APP` の中だけ、web専用ページだけ → **webだけ**。アプリのUIはAPKに組み込まれているので、webのリリースではアプリに届きません。
- `platform::APP` / `#[cfg(feature = "mobile")]` の中だけ → **アプリだけ**。
- 依存・toolchainの更新は普通は両方に効くが、急がない側は次のリリースにまとめてよい。
- 迷ったら両方上げる（不要な更新は「少し使いづらい」程度、出し忘れは不具合）。

## テスト用タグで試す

ワークフローやリリースの形式を変えたときは、バージョンを上げずにテスト用タグで確かめます。`X.Y.Z` はそのコミットの該当Cargo.tomlの版です。

```bash
# アプリ（android.yml）
git tag -a vX.Y.Z-test.N -m "Release workflow test" <コミット>
git push origin refs/tags/vX.Y.Z-test.N
# web（image.yml）
git tag -a web-vX.Y.Z-test.N -m "Release workflow test" <コミット>
git push origin refs/tags/web-vX.Y.Z-test.N
```

- ワークフローはタグが指すコミットのものが使われるので、mainにマージする前のコミットでも試せます。版を上げたときの動きを確かめたいときは、片方のCargo.tomlだけを上げた検証用コミット（mainには入れない）を作り、そこにテスト用タグを打ちます（例 [versioning.md 4](versioning.md#4-検証記録)）。
- アプリのテスト用リリースはプレリリースになるため、`/releases/latest` とアプリの自動更新には出てきません。`image.yml` は動きません。
- webのテスト用タグは `X.Y.Z-test.N` タグのイメージだけを公開します（`X.Y`・`latest` は動きません）。`android.yml` は動かず、リリースも作られません。
- 確認が済んだら、不要なテスト用リリースとタグはForgejoの画面（またはトークンを使うAPI）から削除してください。

ローカルでは `devenv shell -- test-release` で、タグの判定（`scripts/release-version.test.sh`）、変更パスの分類（`scripts/release-scope.test.sh`）と、Forgejo APIのモックに対する公開処理（`scripts/publish-release.test.mjs`）をテストできます。`devenv test` にも含まれます。

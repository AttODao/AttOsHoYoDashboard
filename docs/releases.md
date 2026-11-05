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

タグを打つ前に `devenv test` を通します（CIではテストを実行しません）。版を上げたら `cargo update --workspace --offline` で `Cargo.lock` を追従させます。

### アプリだけ

1. `dashboard-ui/Cargo.toml` の版を `X.Y.Z` に上げ、`Cargo.lock` を更新し、`CHANGELOG.md` に `[アプリ X.Y.Z]` の項目（新しいAPIが要るなら「web A.B.C 以降が必要」）を足した `Release app X.Y.Z` コミットをmainにマージする。
2. 新しいAPIに依存するなら、先にwebをリリース・デプロイしておく（[versioning.md 2.5](versioning.md#25-互換性アプリ--web-api)）。
3. マージコミットに注釈付きタグを付けてpushする。
   ```bash
   git tag -a vX.Y.Z -m "Release app X.Y.Z" <マージコミット>
   git push origin refs/tags/vX.Y.Z
   ```
4. Actionsの `android` だけが動いて成功し、`image` が動かないことを確認する。リリースのアセットは次で確かめられます。
   ```bash
   curl -sS https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard/releases/tags/vX.Y.Z | jq -r '.prerelease, .assets[].name'
   curl -sSLO https://git.attodao.cc/AttODao/AttOsHoYoDashboard/releases/download/vX.Y.Z/ahd-android-X.Y.Z-arm64.apk
   curl -sSLO https://git.attodao.cc/AttODao/AttOsHoYoDashboard/releases/download/vX.Y.Z/SHA256SUMS
   sha256sum -c SHA256SUMS
   ```
5. 失敗したジョブを再実行すると、同じ名前のアセットは置き換えられます。

### webだけ

1. ルート `Cargo.toml` の版を `X.Y.Z` に上げ、`Cargo.lock` を更新し、`CHANGELOG.md` に `[web X.Y.Z]` の項目を足した `Release web X.Y.Z` コミットをmainにマージする。
2. マージコミットに注釈付きタグを付けてpushする。
   ```bash
   git tag -a web-vX.Y.Z -m "Release web X.Y.Z" <マージコミット>
   git push origin refs/tags/web-vX.Y.Z
   ```
3. Actionsの `image` だけが動いて成功し、`android` が動かないこと、`releases/latest` が変わらないことを確認する。
   ```bash
   curl -sS https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard/releases/latest | jq -r .tag_name   # アプリの最新タグのまま
   ```
4. attos-nix-modules の `modules/attos-hoyo-dashboard/nixos.nix` の `image =` 行を `X.Y.Z@sha256:<index digest>` に更新するPR → server-nix の `flake.lock` 更新 → attobox で `nixos-rebuild switch`。

### 両方

1つの `Release app X.Y.Z, web A.B.C` コミットで両方の版を上げ、同じマージコミットに `web-vA.B.C` と `vX.Y.Z` を打ちます。webのデプロイを済ませてからアプリのタグを打つと、互換性の順序（webが先）を守れます。

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

- ワークフローはタグが指すコミットのものが使われるので、mainにマージする前のコミットでも試せます。
- アプリのテスト用リリースはプレリリースになるため、`/releases/latest` とアプリの自動更新には出てきません。`image.yml` は動きません。
- webのテスト用タグは `X.Y.Z-test.N` タグのイメージだけを公開します（`X.Y`・`latest` は動きません）。`android.yml` は動かず、リリースも作られません。
- 確認が済んだら、不要なテスト用リリースとタグはForgejoの画面（またはトークンを使うAPI）から削除してください。

ローカルでは `devenv shell -- test-release` で、タグの判定（`scripts/release-version.test.sh`）、変更パスの分類（`scripts/release-scope.test.sh`）と、Forgejo APIのモックに対する公開処理（`scripts/publish-release.test.mjs`）をテストできます。`devenv test` にも含まれます。

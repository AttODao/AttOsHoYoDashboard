# リリースと配布物

`v*` タグをpushすると、2つのForgejo Actionsワークフローが動きます。

| ワークフロー | 出力 |
| --- | --- |
| `.forgejo/workflows/image.yml` | linux/amd64のコンテナイメージ（Forgejoコンテナレジストリ）。README「リリースイメージ」を参照 |
| `.forgejo/workflows/android.yml` | そのタグのForgejoリリースと、そこに添付するアプリのパッケージ・`SHA256SUMS` |

配布するアプリは現在Android（arm64）だけです。サーバーはコンテナイメージで配布し、リリースのアセットにはしません。

## バージョンとタグ

- 正式リリースのタグは `vX.Y.Z` です。`X.Y.Z` はルートの `Cargo.toml` と `dashboard-ui/Cargo.toml` の `[package] version` の両方と一致しなければなりません。
- テスト用のタグは `vX.Y.Z-<suffix>`（例 `v1.8.0-test.1`、`v1.8.0-rc.1`。suffixは英数字を `.` か `-` でつなげたもの）です。`X.Y.Z` 部分は同じく両方のCargo.tomlと一致させます。テスト用タグのリリースは **プレリリース** として作られます。
- `Y` と `Z` は100未満です。APKの `versionName` は `X.Y.Z`、`versionCode` は `X*10000+Y*100+Z` です（`build-android` が設定します）。
- 判定は `scripts/release-version.sh` が行い、ワークフローは最初のステップでこれを実行します。条件を満たさないタグはビルドの前に失敗します。ビルド後には `aapt2 dump badging` でAPK自身のパッケージ名・`versionCode`・`versionName` がタグと一致することも確認します。

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

- `SHA256SUMS` はそのリリースのパッケージ全部（`SHA256SUMS` 自身を除く）を `sha256sum` の形式（`<64桁の16進数><空白2つ><アセット名>`、1行1ファイル）で列挙します。`sha256sum -c SHA256SUMS` で検証できます。
- AndroidのAPKはリリース鍵（APK Signature Scheme v2/v3）で署名します。Androidは、インストール済みのアプリと同じ証明書で署名されたAPKしか更新としてインストールしません。そのためDioxusのアプリには独自の更新署名（Tauri updaterのような署名ファイル）は作らず、APK署名をそのまま更新の署名として使います。v4署名（`.idsig`、adbの差分インストール用）は作らないので、リリースのアセットはパッケージと `SHA256SUMS` だけです。ワークフローは `dist/` にそれ以外のファイルがあると失敗します。
- リリース鍵の証明書のSHA-256は `dashboard-ui/android/release-signing-cert.sha256` に置いています。ワークフローはAPKの署名者が1つだけで、その証明書がこの値と一致することを確認します。アプリ側で、ダウンロードしたAPKの署名証明書をインストール前に確かめるときもこの値を使えます。各リリースの本文にも同じ値を書きます。
- 署名用のsecret（`ANDROID_KEYSTORE_BASE64`・`ANDROID_KEYSTORE_PASSWORD`・`ANDROID_KEY_ALIAS`）がない場合、正式リリースは失敗します。テスト用タグだけはビルドごとのデバッグ鍵で署名して続け、リリース本文にその旨を書きます（既存のアプリに上書きインストールできません）。
- 鍵を替えると既存のアプリは更新できなくなります。替えるときは証明書ファイルも同じコミットで更新してください。

## 公開の手順（ワークフローが行うこと）

1. タグのリビジョンをcloneし、`scripts/release-version.sh` でバージョンを確認する。
2. `devenv --profile android shell -- build-android --release` でAPKをビルドする。
3. リリース鍵で署名し、署名者の証明書・`versionName`・`versionCode` を確認し、`dist/` に `ahd-android-{version}-arm64.apk` と `SHA256SUMS` を作る。
4. `scripts/publish-release.sh dist` が、タグのリリースがなければ作り（テスト用タグはプレリリース）、`dist/` の全ファイルを添付する。同じ名前の古いアセットは置き換え、それ以外は残す。最後にリリースを読み直し、全アセットの名前とサイズを確認し、公開URLからダウンロードして `sha256sum -c SHA256SUMS` を通す。`SHA256SUMS` に載っていないファイルが `dist/` にあれば何もアップロードせずに失敗する。

## リリースの手順（人が行うこと）

1. `Cargo.toml`・`dashboard-ui/Cargo.toml`・`Cargo.lock` のバージョンを `X.Y.Z` に上げた `Release X.Y.Z` コミットをmainにマージする。タグを打つ前に `devenv test` を通す（CIではテストを実行しません）。
2. マージコミットに注釈付きタグを付けてpushする。
   ```bash
   git tag -a vX.Y.Z -m "Release X.Y.Z" <マージコミット>
   git push origin refs/tags/vX.Y.Z
   ```
3. Actionsの `android` と `image` が成功したことを確認する。リリースのアセットは次で確かめられます。
   ```bash
   curl -sS https://git.attodao.cc/api/v1/repos/AttODao/AttOsHoYoDashboard/releases/tags/vX.Y.Z | jq -r '.prerelease, .assets[].name'
   curl -sSLO https://git.attodao.cc/AttODao/AttOsHoYoDashboard/releases/download/vX.Y.Z/ahd-android-X.Y.Z-arm64.apk
   curl -sSLO https://git.attodao.cc/AttODao/AttOsHoYoDashboard/releases/download/vX.Y.Z/SHA256SUMS
   sha256sum -c SHA256SUMS
   ```
4. 失敗したジョブを再実行すると、同じ名前のアセットは置き換えられます。

## テスト用タグで試す

ワークフローやリリースの形式を変えたときは、バージョンを上げずにテスト用タグで確かめます。

```bash
git tag -a vX.Y.Z-test.N -m "Release workflow test" <コミット>
git push origin refs/tags/vX.Y.Z-test.N
```

- ワークフローはタグが指すコミットのものが使われるので、mainにマージする前のコミットでも試せます。
- リリースはプレリリースになるため、`/releases/latest` とアプリの自動更新には出てきません。
- `image.yml` も動き、`X.Y.Z-test.N` タグのイメージだけを公開します（`X.Y`・`latest` は動きません）。
- 確認が済んだら、不要なテスト用リリースとタグはForgejoの画面（またはトークンを使うAPI）から削除してください。

ローカルでは `devenv shell -- test-release` で、タグの判定（`scripts/release-version.test.sh`）と、Forgejo APIのモックに対する公開処理（`scripts/publish-release.test.mjs`）をテストできます。`devenv test` にも含まれます。

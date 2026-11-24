# 交換コード（ギフトコード）の引き換え

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

AHDの交換コード機能がHoYoverseへ送る引き換え要求の形と、応答の扱いをまとめる。要求の形の根拠は公開されているソース（下表）で、2026-11-24 から本番の応答でも確かめた（「本番での観測」節）。**原神の引き換えは本番で成功した**（retcode `0`）。スターレイルはコード単位の応答（`-2018`、使用済み）までで、成功はまだ見ていない。「Cookie無効」の原因は、hoyolab.com でコピーしてから時間のたった `cookie_token_v2` だと確かめた（番号は `-1071`。HoYoLABでログインし直してコピーしたCookieで引き換えられた）。以下の「未確認」は「実際の応答で確かめていない」という意味。

## 根拠にしたソース

| 内容 | ソース |
| --- | --- |
| 要求の組み立て（`redeem_code`） | genshin.py [`genshin/client/components/hoyolab.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/hoyolab.py) 222〜290行（固定コミット `ab289440`。2026-11-24 時点の `master` [`3439939a`](https://github.com/seriaati/genshin.py/blob/3439939acb91d59875288e875689595667ec87bd/genshin/client/components/hoyolab.py) でも同じ） |
| エンドポイント（`CODE_URL`） | genshin.py [`genshin/client/routes.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/routes.py) 237〜245行 |
| retcodeと例外（`_errors`、`raise_for_retcode`、`check_for_geetest`） | genshin.py [`genshin/errors.py`](https://github.com/seriaati/genshin.py/blob/3439939acb91d59875288e875689595667ec87bd/genshin/errors.py) |
| Geetestのretcode（`GEETEST_RETCODES`）、対応ゲーム（`CODE_SUPPORTED_GAMES`） | genshin.py [`genshin/constants.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/constants.py) |
| 必要なCookie（`requires_cookie_token`）と、拒まれたときのstokenからの再発行 | genshin.py [`genshin/client/manager/managers.py`](https://github.com/seriaati/genshin.py/blob/3439939acb91d59875288e875689595667ec87bd/genshin/client/manager/managers.py) 571〜600行、[`base.py`](https://github.com/seriaati/genshin.py/blob/3439939acb91d59875288e875689595667ec87bd/genshin/client/components/base.py) `_refresh_cookie_token`（コミット [`4af76efc`](https://github.com/seriaati/genshin.py/commit/4af76efc396eeedbb4a8e6ef4402adeb5a0397e4)、2026-11-09） |
| 公式のスターレイル交換ページの要求 | `https://hsr.hoyoverse.com/gift` が読み込む `https://gs.hoyoverse.com/lib/web-cdkey-exchange/hkrpg/main.js`（2026-11-24 にログインせずに取得した公開スクリプト。`webExchangeCdkeyRisk`・`getRiskParams`・`CDKEY_LANG_MAP`・`CDKEY_API_CONFIG`） |
| 引き換えに送るCookieと、Cookie無効の扱い | hoyolab-auto [`hoyolab-modules/starrail/redeem-code.js`](https://github.com/torikushiii/hoyolab-auto/blob/86a52f4c163f4413db3dac53107a3713e3ada8df/hoyolab-modules/starrail/redeem-code.js)、[`redemption-request.js`](https://github.com/torikushiii/hoyolab-auto/blob/86a52f4c163f4413db3dac53107a3713e3ada8df/hoyolab-modules/redemption-request.js)、[`README.md`](https://github.com/torikushiii/hoyolab-auto/blob/86a52f4c163f4413db3dac53107a3713e3ada8df/README.md) 180〜187行 |
| Cookie無効のときの扱い（Hoyo Buddy） | [`hoyo_buddy/hoyo/clients/gpy.py`](https://github.com/seriaati/hoyo-buddy/blob/a45f2069211790cfbfe98f673d9cad14fbf2560c/hoyo_buddy/hoyo/clients/gpy.py) 694〜790行、[`l10n/en_US.yaml`](https://github.com/seriaati/hoyo-buddy/blob/a45f2069211790cfbfe98f673d9cad14fbf2560c/l10n/en_US.yaml) 390〜393行 |
| `cookie_token_v2` の寿命 | HoYoAPI [`README.md`](https://github.com/vermaysha/hoyoapi#about-a-cookietokenv2)「About a cookieTokenV2」 |

## 要求の形（原神は確認済み、スターレイルは受け付けまで確認）

| | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| メソッド | `GET` | `POST`（JSON本文、`Content-Type: application/json`） |
| URL | `https://public-operation-hk4e.hoyoverse.com/common/apicdkey/api/webExchangeCdkey` | `https://public-operation-hkrpg.hoyoverse.com/common/apicdkey/api/webExchangeCdkeyRisk` |
| パラメーター | クエリ `uid`, `region`, `lang`, `cdkey`, `game_biz=hk4e_global` | 本文 `cdkey`, `device_uuid`（毎回ランダムなUUID）, `game_biz=hkrpg_global`, `lang`, `platform="4"`, `region`, `t`（Unixミリ秒）, `uid`（ロール一覧の `game_uid` の文字列） |
| ヘッダー | `Origin` / `Referer` = `https://genshin.hoyoverse.com` / `https://genshin.hoyoverse.com/` | `Origin` / `Referer` = `https://hsr.hoyoverse.com` / `https://hsr.hoyoverse.com/`、`x-rpc-language: ja` |

- スターレイルの要求は、genshin.py と公式ページのスクリプトが3点で違う。AHDは公式ページに合わせた（2026-11-24。サーバーが自分のページの送る形を受け付けるのは確かなので）：`t` は genshin.py の `int(time.time())`（秒）ではなく `Date.now()`（ミリ秒）、`uid` は数値ではなく文字列、`x-rpc-language` ヘッダーを付ける（genshin.py は付けない）。ホスト・パス・本文の他の項目（`platform` はデスクトップで `"4"`、`device_uuid`、`lang` の短縮形）は両者で同じ。この変更の後、スターレイルの要求にコード単位の応答（`-2018`、使用済み）が返ったので、サーバーはこの形を受け付けている（2026-11-24。成功の応答はまだ見ていない）。Cookie無効の応答の原因はこの3点ではなかった（「本番での観測」節）。
- `region` はロール一覧（`getUserGameRolesByCookie`）が返したserver（例 `os_asia`、`prod_official_asia`）をそのまま使う。公式ページも同じAPIのロール一覧から `region` を取る。genshin.pyはUIDの先頭の数字から推定する（スターレイルは 6→`prod_official_usa`、7→`prod_official_eur`、8→`prod_official_asia`、9→`prod_official_cht`）。
- `lang` は genshin.py の `create_short_lang_code`（`ja-jp` → `ja`）と公式ページの `CDKEY_LANG_MAP` にならい `ja`。
- DSヘッダーは送らない（genshin.py・公式ページとも送らない）。User-Agentは他の要求と同じ値。共有のHTTPクライアント（`shared_http`）は既定ヘッダーがUser-Agentだけで、Cookieストアを持たず（reqwestの `cookies` 機能は無効）、リダイレクトは追わない。
- 原神の公式ページ（`genshin.hoyoverse.com/en/gift`）は同じホスト `public-operation-hk4e.hoyoverse.com` を使うが、要求を作るスクリプトの部分は確かめていない。原神の要求は genshin.py のまま。
- 実装は [`src/hoyolab/client/redemption.rs`](../../src/hoyolab/client/redemption.rs)。要求の形はローカルのTLSモック（`redemption_tests.rs`）で本文まで照合している。

### 依頼にあった想定との違い

カード作成時の想定（genshin.pyの記憶）とソースが食い違った点。ソースに従った。

- ホストは `sg-hk4e-api.hoyoverse.com` / `sg-hkrpg-api.hoyoverse.com` ではなく `public-operation-hk4e.hoyoverse.com` / `public-operation-hkrpg.hoyoverse.com`（genshin.py と公式ページ。hoyolab-auto は今も `sg-*-api` を使う）。
- スターレイルはクエリのGETではなく、`device_uuid`・`platform`・`t` を含むJSON本文のPOST。
- retcodeは想定より多い（次節）。

## Cookie

- 送るのは `cookie_token_v2=…; account_id_v2=…[; account_mid_v2=…]` だけ。hoyolab-auto は引き換えの要求に `cookie_token_v2`・`account_mid_v2`・`account_id_v2`（と旧形式の `cookie_token`・`account_id`）だけを送るので、この組み合わせで足りるというのが公開ソースの示すところで、本番でも原神の引き換えがこの3つだけで成功した（2026-11-24）。genshin.py はCookie全体を送る。`ltoken_v2` などは送らず、逆に読み取りやログインボーナスの要求に `cookie_token_v2` は送らない。
- `account_mid_v2` は登録されたCookieにあればその値をそのまま送り、ない（使えない文字を含む）ときだけ `ltmid_v2` を使う（2026-11-24 から。それまでは常に `ltmid_v2`）。HoYoLABでは通常同じ値（hoyolab-auto はstokenで更新したとき `account_mid_v2` に `ltmid_v2` を入れる）。
- `account_id_v2` が `ltuid_v2`（本人確認で検証するUID）と違う、ない、重複するCookieは登録を拒む。`cookie_token_v2` そのものがそのアカウントのものかをHoYoLABへ問い合わせる読み取り専用のAPIはgenshin.pyにない（`fetch_cookie_token_info` はCookieの更新を兼ねるので使わない）。
- 旧形式の `cookie_token`（v1）は保存しない。
- 保存は他のCookieと同じAES-256-GCMで、ownerと検証済みUIDに束縛する（[`src/dashboard/hoyolab/credentials/`](../../src/dashboard/hoyolab/credentials/)）。保存した値は貼り付けた値と1バイトも変えない（前後の空白と `Cookie:` を除くだけ。`v2_` で始まり `=`・`-`・`_`・`.`・`/`・`+`・`%` を含む値の往復はテスト済み）。引用符・`,`・`\` を含む値は変えずに拒む。APIは「引き換え用の資格情報がある」という真偽値（`redemption_credentials`）だけを返す。

### `cookie_token_v2` の寿命（古くなることは確認、長さは未確認）

公開ソースは、ブラウザからコピーした `cookie_token_v2` が `ltoken_v2` より先に使えなくなることを示している。本番でもそのとおりだった（2026-11-24。以前に登録したCookieの `cookie_token_v2` は `-1071` で拒まれ、ログインし直してコピーしたCookieでは引き換えられた。`ltoken_v2` の読み取りはその間も動いていた）。**何時間・何日で使えなくなるかは分かっていない**（下のソースにも値はない）。

- genshin.py は 2026-11-09（`4af76efc`）から、引き換えなど `requires_cookie_token` の要求がCookie無効（`InvalidCookies`）で拒まれると、Cookieに `stoken` があればそれで `cookie_token_v2` を作り直して1度だけ送り直す。
- Hoyo Buddy はその作り直しにも失敗した応答を「Cookie Token Expired」と表示し、交換コードはメールアドレスとパスワードでログインした（`stoken` のある）利用者だけが使える、と案内する。DevToolsでコピーするログイン方法では `cookie_token_v2` を受け取らない。
- hoyolab-auto はCookieに `stoken` があれば起動時と2時間ごとに `cookie_token_v2` を作り直し、`-1071` を一時的な失敗として扱う（コードを待ちに残す）。
- HoYoAPI は `cookie_token_v2` は寿命が短く、各ゲームの交換ページ（例 `genshin.hoyoverse.com/en/gift`）で取れる、と書く。
- AHDは `stoken` を受け取らず、Cookieを自分で作り直さない（spec §9）。だから案内は「Cookie無効と出たら、HoYoLABでログアウトしてログインし直し、すぐにコピーして登録し直す」で、これで直ることを本番で確かめた（2026-11-24）。hoyolab.com でコピーしたCookieで引き換えられるので、交換ページのCookieや `stoken` は要らない。hoyolab.com でコピーした値と交換ページの値が同じかは確かめていない。

## 応答の解釈（`0`・`-2017`・`-2018`・`-1071` は本番で確認）

genshin.py の `_request` と同じく、まず `check_for_geetest`（retcodeがGeetestのもの、または `data`（`data.gt_result` があればそれ）が `risk_code != 0` かつ `gt` と `challenge` が空でなく `success != 0`）を見て、次にretcodeを見る。**すべての応答のretcode（数値だけ）を台帳の `retcode` に保存し**、INFOのログ行 `redemption code answered` に `retcode=` として出し、`GET /code-history`（`retcode`）と `GET /code-redemption`（止めた応答の `action_retcode`）で返す。利用者の画面（交換コードのタブ）は番号を表示しない（web・アプリ 2.3.2 から。2.3.1 では原因を調べるために履歴の行と要対応の案内に出していた）。番号を見るのは運用者で、ログと `/metrics` だけ。APIのフィールドは公開済みのアプリ（2.3.1）が読むので残す。メッセージと `data` は保存もログ出力もしない。通信エラー・読めない応答は `retcode` なし（ログは `retcode=none`）。

| retcode | genshin.py | AHDの分類 | 記録する状態 | 範囲 |
| --- | --- | --- | --- | --- |
| `0` | 成功 | 引き換え済み | `redeemed` | UID |
| `-2017`, `-2018` | `RedemptionClaimed` | 使用済み | `already_used` | UID |
| `-2001`（期限切れ）, `-2006`（上限） | `RedemptionInvalid` | 期限切れ | `expired` | **全員** |
| `-1065`, `-2003`（形式）, `-2004` | `RedemptionInvalid` | 無効 | `invalid` | **全員** |
| `-2008`（地域）, `-2014`（未有効） | `RedemptionRegionLock` / `RedemptionInvalid` | このアカウントでは使えない | `invalid` | UID |
| `-2011`, `-2021` | `RedeemGameLevelTooLow` | ランク不足 | `rank_too_low`（1日1回まで再試行） | UID |
| `-1071`, `-100`, `10001` | `InvalidCookies` | Cookie無効 | `cookie_invalid`（要対応） | 利用者 |
| `-1073` | `AccountNotFound`（"Account has no game account bound to it."） | UIDが見つからない | `no_game_account` | **UID** |
| `10035`, `5003`, `10041`, `1034`、`data` のリスク判定 | `GeetestError` / `DailyGeetestTriggered` | CAPTCHAが必要 | `captcha_required`（要対応） | 利用者 |
| `-2016`, `-110`, `1028` | `RedemptionCooldown` / `VisitsTooFrequently` | 待ち | `pending`（待ってから再試行） | — |
| それ以外 | — | 不明 | `unknown`（自動では再送しない） | UID |
| 通信エラー・タイムアウト・HTTPエラー・JSONでない | — | 不明 | `unknown`（自動では再送しない） | UID |

- `-1073`（2026-11-24 から）: genshin.py の表では `InvalidCookies` ではなく `AccountNotFound` で、`requires_cookie_token` の作り直しの対象にもならない（`raise_for_retcode` のdocstringは `-107x = invalid cookies` とまとめて書くが、実際の対応表は別の例外）。UIDやserverの取り違えでも起こり得るので、利用者全体を止めない。そのUIDはその実行の残りを送らず、次の実行からも計画から外し、Cookieの更新か「今すぐ引き換え」で `pending` に戻す（`POST /code-redemption/run` の対象にするため `GET /code-redemption` の `pending` に数える）。他のUIDは続ける。それまでは `cookie_invalid` として利用者全体を止めていた。
- `-100` と `10001`: genshin.py は引き換えに限らず `InvalidCookies` とし、hoyolab-auto も `-100` を「Please log in」の一時的な失敗として扱う。引き換えで別の意味を持つという根拠はないので、Cookie無効のままにする（番号はログと `/metrics` で区別できる）。
- genshin.py はクールダウンの間隔を書いていない。待ちの応答の後は30秒、通常は同じHoYoLABアカウントへの要求を7秒あける（約5秒とされる間隔に余裕を足した値。hoyolab-auto は6秒）。
- CAPTCHAは解かない（§9）。要対応になった利用者には、Cookieの更新か「今すぐ引き換え」まで要求を送らない。
- `/metrics` は応答の分類（`ahd_code_redemptions_total{game,outcome}`、`no_game_account` を含む）に加えて、Cookie無効の応答を番号ごとに数える（`ahd_code_redemption_cookie_retcodes_total{game,retcode}`、`retcode` は `-1071`・`-100`・`10001`）。

## 本番での観測

| 日時（JST） | 内容 | 分かったこと |
| --- | --- | --- |
| 2026-11-24 15:39:49・15:42:50・15:42:57 | web 2.3.0。スターレイル、手入力のコード1件。3回とも `outcome="cookie_invalid"`。15:42の2回は、それぞれ利用者が引き換えを再開した後（Cookieの登録し直しか「今すぐ引き換え」。止まっている間は送らないので、コードからそう推定できる）。原神は試していない。 | 応答はCookie無効の分類（`-1071`・`-1073`・`-100`・`10001` のどれか。当時は番号を保存していなかった）。Cookieを登録し直しても同じ応答が返った可能性が高い。 |
| 2026-11-24、web 2.3.1 の反映（18:01）の後、1件目 | スターレイル、手入力。`outcome="cookie_invalid" retcode="-1071"`。Cookieは以前に登録したままのもの。 | 番号は `-1071`（genshin.py の `InvalidCookies`、hoyolab-auto のテストでは「Not logged in」）。`-1073`・`-100`・`10001` ではない。 |
| 同日、続けて（利用者がHoYoLABでログアウトしてログインし直し、すぐにコピーしたCookieを登録した後。ログの順） | スターレイル手入力 `already_used`（`-2018`）、原神手入力 `redeemed`（`0`）、原神の一覧 `redeemed`（`0`）×3、原神の一覧 `already_used`（`-2017`）×2、スターレイルの一覧 `already_used`（`-2018`） | 新しいCookieでは `-1071` が出ず、原神は引き換えられた。原因は古くなった `cookie_token_v2` で確定。原神のGETの形と、送る3つのCookie（`cookie_token_v2`・`account_id_v2`・`account_mid_v2`）はこれで足りる。スターレイルのPOSTはコード単位の応答が返ったので受け付けられている（成功はまだ見ていない）。 |

## 原因の候補（2026-11-24 の調査と、同日の本番の応答での判定）

| 候補 | 判定 | 根拠・次に分かること |
| --- | --- | --- |
| **hoyolab.com でコピーした `cookie_token_v2` が古くなった** | **原因（本番で確定）** | 番号は `-1071`。ログインし直してすぐコピーしたCookieを登録すると、`-1071` は出ず原神で引き換えられた（「本番での観測」節）。hoyolab.com のCookieで引き換えられるので、`stoken`（§9の変更）や交換ページのCookieは要らない。寿命の長さは未確認。 |
| 送るCookieの組み合わせが足りない（`ltoken_v2` なども要る） | 否定（ソースと本番の成功から） | hoyolab-auto は `cookie_token_v2`・`account_mid_v2`・`account_id_v2` だけを送り、AHDもこの3つだけで原神の引き換えに成功した。 |
| UID・serverの取り違え（`-1073`） | 否定（本番で） | 番号は `-1073` ではなかった。新しいCookieでは、スターレイル・原神ともコード単位の応答（使用済み・成功）が返った。 |
| 公式ページとの要求の違い（`t` の単位、`uid` の型、`x-rpc-language`） | 原因ではない（本番で） | 公式ページに合わせた後も、古いCookieでは同じ `-1071` だった。合わせた形のスターレイルの要求には `-2018` が返ったので、この形は受け付けられている。 |
| 保存したCookieの値が貼り付けた値と違う | 否定（テストで） | 往復テスト（`credentials/parser/tests.rs`、`verification_tests.rs`）。 |
| 共有HTTPクライアントが何かを足す・落とす | 否定（コードから） | 既定ヘッダーはUser-Agentだけ、Cookieストアなし、リダイレクトなし。 |
| Cookieの更新が保存されない・止まったままになる | 否定（テストで） | 止まっている間のCookie更新で、引き換え用のCookieが置き換わり、`credential_version` が上がり、止まりが解け、止めたコードが `pending` に戻る（`verification_tests.rs`、`browser.mjs`）。 |

## 試していないこと

- スターレイルの引き換えの成功（POST）。受け付け（`-2018`）までは確かめた。原神の成功（GET）は確かめた。
- `cookie_token_v2` が何時間・何日で使えなくなるか。交換ページのCookieと hoyolab.com のCookieが同じか。
- 成功時の `data` の形（報酬の文は取得元の一覧の値を使い、応答からは読まない）。
- 本番での観測のうち、使用済み・成功以外の応答（期限切れ・無効・ランク不足・CAPTCHA・UIDが見つからない）。分類は公開ソースのまま。

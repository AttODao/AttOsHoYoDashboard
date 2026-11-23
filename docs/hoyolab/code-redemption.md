# 交換コード（ギフトコード）の引き換え

[HoYoLAB API 調査の入口](../hoyolab-api.md)へ戻る。

AHDの交換コード機能（第2波）がHoYoverseへ送る引き換え要求の形と、応答の扱いをまとめる。根拠はすべて参照実装 genshin.py の固定したコミット [`ab289440`](https://github.com/seriaati/genshin.py/tree/ab289440508ef81a9672a198a53b9a31f9365c4a) のソースで、**実際の応答で確かめたものは1つもない**（開発中・リリース前に本物のアカウントで引き換えを一度も行っていない）。以下の「未確認」はその意味。

## 根拠にしたソース

| 内容 | ファイル |
| --- | --- |
| 要求の組み立て（`redeem_code`） | [`genshin/client/components/hoyolab.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/hoyolab.py) 222〜290行 |
| エンドポイント（`CODE_URL`） | [`genshin/client/routes.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/routes.py) 237〜245行 |
| retcodeと例外（`_errors`、`raise_for_retcode`、`check_for_geetest`） | [`genshin/errors.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/errors.py) |
| Geetestのretcode（`GEETEST_RETCODES`）、対応ゲーム（`CODE_SUPPORTED_GAMES`） | [`genshin/constants.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/constants.py) |
| 必要なCookie（`requires_cookie_token`）、送信と応答の解釈（`_request`） | [`genshin/client/manager/managers.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/manager/managers.py) |
| JSON本文として送ること（`request` が `data` を `json=` で渡す）、User-Agent | [`genshin/client/components/base.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/client/components/base.py) |
| `game_biz`・言語の短縮・UIDからのserver推定 | [`genshin/utility/uid.py`](https://github.com/seriaati/genshin.py/blob/ab289440508ef81a9672a198a53b9a31f9365c4a/genshin/utility/uid.py) |

## 要求の形（未確認）

| | 原神 | 崩壊：スターレイル |
| --- | --- | --- |
| メソッド | `GET` | `POST`（JSON本文、`Content-Type: application/json`） |
| URL | `https://public-operation-hk4e.hoyoverse.com/common/apicdkey/api/webExchangeCdkey` | `https://public-operation-hkrpg.hoyoverse.com/common/apicdkey/api/webExchangeCdkeyRisk` |
| パラメーター | クエリ `uid`, `region`, `lang`, `cdkey`, `game_biz=hk4e_global` | 本文 `cdkey`, `device_uuid`（毎回ランダムなUUID）, `game_biz=hkrpg_global`, `lang`, `platform="4"`, `region`, `t`（Unix秒）, `uid`（数値） |
| `Origin` / `Referer` | `https://genshin.hoyoverse.com` / `https://genshin.hoyoverse.com/` | `https://hsr.hoyoverse.com` / `https://hsr.hoyoverse.com/` |

- `region` はロール一覧（`getUserGameRolesByCookie`）が返したserver（例 `os_asia`、`prod_official_asia`）をそのまま使う。genshin.pyはUIDの先頭の数字から推定するが、AHDは保存済みのロールの値を使う。
- `lang` は genshin.py の `create_short_lang_code`（`ja-jp` → `ja`）にならい `ja`。
- DSヘッダーと `x-rpc-*` は送らない（genshin.pyも送らない）。User-Agentは他の要求と同じ値。
- 実装は [`src/hoyolab/client/redemption.rs`](../../src/hoyolab/client/redemption.rs)。要求の形はローカルのTLSモック（`redemption_tests.rs`）で本文まで照合している。

### 依頼にあった想定との違い

カード作成時の想定（genshin.pyの記憶）とソースが食い違った点。ソースに従った。

- ホストは `sg-hk4e-api.hoyoverse.com` / `sg-hkrpg-api.hoyoverse.com` ではなく `public-operation-hk4e.hoyoverse.com` / `public-operation-hkrpg.hoyoverse.com`。
- スターレイルはクエリのGETではなく、`device_uuid`・`platform`・`t` を含むJSON本文のPOST。
- retcodeは想定より多い（次節）。`-2006`（使用回数の上限）、`-2008`（地域）、`-2014`（未有効）、`-2021`（ランク）、`-1065`（無効）、`-1073`（ゲームアカウントなし）、Geetestの `10035`・`5003`・`10041`・`1034` と、`data` 内のリスク判定（`gt_result`）がある。

## Cookie

- genshin.py の `requires_cookie_token` は `cookie_token` または `cookie_token_v2` を要求し、Cookie全体を送る。どの名前が本当に必要かはソースからは決まらない（未確認）。
- AHDは、新しく登録されたCookieに `cookie_token_v2` があれば、同じHoYoLABアカウントの `account_id_v2` を必須として一緒に保存し、引き換えには `cookie_token_v2=…; account_id_v2=…[; account_mid_v2=…]` だけを送る。`ltoken_v2` などは送らず、逆に読み取りやログインボーナスの要求に `cookie_token_v2` は送らない。`account_mid_v2` には `ltmid_v2` と同じMIDを使う。
- `account_id_v2` が `ltuid_v2`（本人確認で検証するUID）と違う、ない、重複するCookieは登録を拒む。`cookie_token_v2` そのものがそのアカウントのものかをHoYoLABへ問い合わせる読み取り専用のAPIはgenshin.pyにない（`fetch_cookie_token_info` はCookieの更新を兼ねるので使わない：§9のCookie自動更新はしない）。拘束は引き換え時にHoYoLABが判断する（未確認）。
- 旧形式の `cookie_token`（v1）は保存しない。
- 保存は他のCookieと同じAES-256-GCMで、ownerと検証済みUIDに束縛する（[`src/dashboard/hoyolab/credentials/`](../../src/dashboard/hoyolab/credentials/)）。APIは「引き換え用の資格情報がある」という真偽値（`redemption_credentials`）だけを返す。

## 応答の解釈（未確認）

genshin.py の `_request` と同じく、まず `check_for_geetest`（retcodeがGeetestのもの、または `data`（`data.gt_result` があればそれ）が `risk_code != 0` かつ `gt` と `challenge` が空でなく `success != 0`）を見て、次にretcodeを見る。メッセージと `data` は保存・ログ出力しない。

| retcode | genshin.py | AHDの分類 | 記録する状態 | 範囲 |
| --- | --- | --- | --- | --- |
| `0` | 成功 | 引き換え済み | `redeemed` | UID |
| `-2017`, `-2018` | `RedemptionClaimed` | 使用済み | `already_used` | UID |
| `-2001`（期限切れ）, `-2006`（上限） | `RedemptionInvalid` | 期限切れ | `expired` | **全員** |
| `-1065`, `-2003`（形式）, `-2004` | `RedemptionInvalid` | 無効 | `invalid` | **全員** |
| `-2008`（地域）, `-2014`（未有効） | `RedemptionRegionLock` / `RedemptionInvalid` | このアカウントでは使えない | `invalid` | UID |
| `-2011`, `-2021` | `RedeemGameLevelTooLow` | ランク不足 | `rank_too_low`（1日1回まで再試行） | UID |
| `-1071`, `-100`, `10001`, `-1073` | `InvalidCookies` / `AccountNotFound` | Cookie無効 | `cookie_invalid`（要対応） | 利用者 |
| `10035`, `5003`, `10041`, `1034`、`data` のリスク判定 | `GeetestError` / `DailyGeetestTriggered` | CAPTCHAが必要 | `captcha_required`（要対応） | 利用者 |
| `-2016`, `-110`, `1028` | `RedemptionCooldown` / `VisitsTooFrequently` | 待ち | `pending`（待ってから再試行） | — |
| それ以外 | — | 不明 | `unknown`（自動では再送しない） | UID |
| 通信エラー・タイムアウト・HTTPエラー・JSONでない | — | 不明 | `unknown`（自動では再送しない） | UID |

- genshin.py はクールダウンの間隔を書いていない。待ちの応答の後は30秒、通常は同じHoYoLABアカウントへの要求を7秒あける（約5秒とされる間隔に余裕を足した値。未確認）。
- CAPTCHAは解かない（§9）。要対応になった利用者には、Cookieの更新か「今すぐ引き換え」まで要求を送らない。

## 試していないこと

- 本物のアカウントでの引き換え（POST・GETとも）。本番で最初の引き換えが、この表の想定どおりに分類されるかは [運用文書](../dashboard-operations.md) の手順で確かめる。
- `cookie_token_v2` だけで足りるか、`ltoken_v2` も要るか。
- 成功時の `data` の形（報酬の文は取得元の一覧の値を使い、応答からは読まない）。

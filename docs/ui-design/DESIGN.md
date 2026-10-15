# AGD / R3 visual contract

Surface mode: **Operate**。指定の[NFT/Web3参照](https://www.uupm.cc/demo/nft-web3)から世界観を採用し、ゲーム状態を読む・設定する用途へ翻訳する。旧Orbit Folio／Arcade IndexはR1の履歴として扱う。R3は本実装の基準としてユーザー承認済み。本書はドラフトのvisual contractであり、[本番実装・検証](production-r3.md)とは区別する。

## 配色

ドラフト値の正本：[`preview/style.css`](preview/style.css)の`:root`とlight用semantic tokens。本番の配信元は`dashboard-ui/assets/dashboard.css`。下表はdarkの基準。dark／lightは背景・本文・操作部まで一緒に切り替え、両方で本文4.5:1・意味のある境界3:1を保つ。R2を保存し、同じR3内の小改修として扱う（R4ではない）。

| Primitive / semantic | 値 | 役割 |
|---|---|---|
| canvas | `#0C0B0E` | 黒に近い背景 |
| surface / elevated | `#18171D` / `#24222C` | カード／操作部 |
| text / muted | `#F8F7FC` / `#B7B2C4` | 本文／補足 |
| line / decorative | `#7F778E` / `#38333F` | 入力・ボタン境界／装飾的な区切り |
| pink / cyan | `#FF58EC` / `#49EEFF` | 主要操作のグラデーション／フォーカス |
| on-accent | `#0B0B10` | グラデーション上の文字 |
| mint / lavender | `#93F5CE` / `#D5B7FF` | 原神／スターレイル |
| warning / danger | `#FFD585` / `#FFB2BD` | 未完了・不明／エラー・削除 |

参照の純色グラデーションを少し明るくして、ボタンの黒文字が全区間で4.5:1以上になるようにする。境界に意味がある操作部は3:1以上。カードの細い暗い境界は装飾であり、入力境界へ転用しない。ゲーム別の色に加え、見出し・文字でも区別する。光り続けるグロー、グリッチ、スキャンラインは使わない。

## Type / spacing / controls

- ブランドは**AGD**、Orbitron 700・26px。大きな数値もOrbitron、44–64px。日本語本文はNoto Sans JP・16px、補足14px、セクション22px、画面見出し28–36px、行間1.6（見出し1.45）。
- Max content 1280px、実体の左右padding24px。1440pxでカード領域約1232px。Mobileはpadding20px。サイドバーは0px。
- Desktop Dashboardは2ゲームを等幅2列・gap24pxで同時表示。カード全面を原神`#132B27`／スターレイル`#27223E`に分け、補足色も`#B7D9CC`／`#CDC3E7`へ寄せる。900px以下はゲームタブで1枚ずつ表示し、ON/OFFは各カードの上部に置く。
- Settingsは5つのタブで内容を置き換える。Mobileは「ゲーム／ボーナス／連携／認証／削除」に短縮し、ARIA名は完全な項目名を保つ。選択線・文字色で現在位置を示し、矢印／Home／Endとroving tabindexに対応する。時間は大きな時・分のselect、下にタイムゾーン`+09:00`を残す。
- ログインは幅480pxの中央カードだけ。パスワード欄から送信まで24px以上空ける。左の紹介欄・送信下の説明は置かない。通常UIの補助説明は削除し、ラベル・データ・状態／エラーだけを残す。
- Card radius16px、Button/Input radius8px。操作高さ48px。タブも44px以上。スイッチの見た目のトラックは48×28px、ラベル全体の当たり判定は48px高。
- 唯一の上部メニューは文字なしの48pxハンバーガー。右端・全高の304px drawer（狭幅では画面内）、暗いbackdropで本文を動かさない。native `dialog`でモーダルフォーカスを管理し、明示X／外側クリック／Escapeで閉じてトリガーへ戻す。280msのtransform＋display／overlayのdiscrete close、reduced-motionでは即時。
- 参照`https://artifact-diagnoser.ktonk.com/`は実Chromiumの1440／390pxで観察：右全高304px、暗いbackdrop、本文移動なし、右から約300ms、外側／Escapeで閉じる。参照はgearでXなし、AGDはハンバーガーを維持してXを足す。参照の正確なCSS duration／easingを取得した主張ではない。
- 日本語／英語とdark／lightの操作は**メニュー内だけ**。テーマは背面の製品画面も変更し、言語は閉じずに即時反映。英語はメニューだけでなく製品画面全体が対象。ログイン／制限中の初期設定にはメニューを出さず保存済み設定を継承し、完了前のログアウトのみのゲートを維持する。
- 全パスワード入力に48pxの目アイコン。入力label／ID、対象の`aria-controls`、`aria-pressed`、翻訳済みの表示／非表示名を持つ。空・read-onlyの合成入力のtypeだけを切り替え、値getterは使わない。再render／window blur／document hidden／メニューを開く時に再マスク。`localStorage`は`agd-preview-lang`／`agd-preview-theme`だけで、秘密・表示状態は保存しない。
- Focus2px cyan +4px offset。文字選択・caretも配色に合わせる。Switchは120msの短い状態変化のみ。reduced-motionでは即時。
- 数字の上限は合成レスポンスの例であり、200／300を製品定数にしない。NFT画像・ウォレット・取引機能はこの画面に含めない。

## Assets / verification boundary

OrbitronとNoto Sans JPは公開Google Fontsをプレビューだけで読み込む。読込不能時はfallback。承認後の本番では配信・ライセンス・CSP・最小サブセットを別途確認する。ここで本番CSPや依存関係を変更しない。

Iconsは20pxの少数path SVG、同じstroke1.8px。ゲームの写真・公式画像・ロゴ素材は未使用。HTMLプレビューは合成動作の参照。実際の表示切替・motion・設定保存はブラウザ資料だけで、本番実装ではない。Nativeの目は静的な編集用アイコンでフォームの秘密を表示しない。Nativeメニューは言語／テーマの代表状態へリンクできるが、全role・全ページの組合せや設定永続化を再現しない。最新の保存・検証範囲とライト画像確認省略は`revision-3.md`を参照。

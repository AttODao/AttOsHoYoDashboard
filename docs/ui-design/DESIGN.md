# AGD / R2 visual contract

Surface mode: **Operate**。指定の[NFT/Web3参照](https://www.uupm.cc/demo/nft-web3)から世界観を採用し、ゲーム状態を読む・設定する用途へ翻訳する。旧Orbit Folio／Arcade IndexはR1の履歴として扱う。これはAI制作案であり、最終視覚承認ではない。

## 配色

値の正本：[`preview/style.css`](preview/style.css)の`:root`。Penpot R2への登録は未実施。

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
- 主領域とボーナスの比率1.8:1、gap24px。900px以下は1カラム。フォームは2列から600px以下で1列へ。設定内のセクションは自然な可変高さで、見出し・補足を固定y位置に置かない。
- Card radius16px、Button/Input radius8px。操作高さ48px。設定内の目次も44px以上。スイッチの見た目のトラックは48×28px、ラベル全体の当たり判定は48px高。
- 唯一の上部メニューを主要色で示す。開いたメニューは272px、Mobileは画面幅内に収める。Escapeで閉じてトリガーへフォーカスを戻す。
- Focus2px cyan +4px offset。文字選択・caretも配色に合わせる。Switchは120msの短い状態変化のみ。reduced-motionでは即時。
- 数字の上限は合成レスポンスの例であり、200／300を製品定数にしない。NFT画像・ウォレット・取引機能はこの画面に含めない。

## Assets / verification boundary

OrbitronとNoto Sans JPは公開Google Fontsをプレビューだけで読み込む。読込不能時はfallback。承認後の本番では配信・ライセンス・CSP・最小サブセットを別途確認する。ここで本番CSPや依存関係を変更しない。

Iconsは20pxの少数path SVG、同じstroke1.8px。ゲームの写真・公式画像・ロゴ素材は未使用。HTMLプレビューは後続Penpot制作の参照であり、ネイティブ部品の共有編集性を証明するものではない。

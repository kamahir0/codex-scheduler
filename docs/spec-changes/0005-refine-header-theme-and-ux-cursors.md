# 仕様変更: ヘッダー表示修正・テーマ3択化・ウィンドウサイズとカーソル管理（UI Refinement & Theme Options）

Status: Applied

## Affected Specifications

- `docs/gui/app-shell.md`: SHELL-LAYOUT-001, SHELL-THEME-001, SHELL-BRANDING-001
- `apps/gui/src/App.tsx`: Header, Theme state, Dropdown, Cursor
- `apps/gui/src/components/MetricCards.tsx`: Remove hoverable, cursor default
- `apps/gui/src-tauri/tauri.conf.json`: Window minimum sizes

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - ヘッダの文字が切れている不具合の解消。
  - `Tauri 2` 等の技術スタック表示タグの削除。
  - 左上ロゴに公式アプリアイコンを適用。
  - テーマ切り替えを「システム設定追従」「ライト」「ダーク」の3択化し、ホバードロップダウンで選択可能にする。
  - レスポンシブが崩れないようウィンドウ最小サイズ（minWidth/minHeight）の適切な引き上げ。
  - クリックできない要素でポインターカーソル（指マーク）が出ないようカーソル管理を徹底。
- **Agent Decision**:
  - Ant Design Header のデフォルト `line-height: 64px` を `normal` に上書きし、ブランドタイトルを精密配置の `div` に変更して文字切れを根本解消。
  - MetricCards の `hoverable` を削除し、情報表示カードでの不要な指マークを排除。
  - `window.matchMedia` によるシステム外観の動的リスナーを組み込み、OSのダーク/ライトモード切り替えへリアルタイム追従。
  - ウィンドウサイズを `1140x740`（最小 `960x620`）に設定。

## 提案する差分（Proposed Delta）

- `apps/gui/src/App.tsx`: Headerコンポーネントの構造改善、ThemeMode（system/light/dark）ステートとDropdownの導入。
- `apps/gui/src/components/MetricCards.tsx`: `hoverable` の除去、`cursor: default` の明示。
- `apps/gui/src/index.css`: `.ant-layout-header` および `.ant-card` への CSS ガード追加。
- `apps/gui/src-tauri/tauri.conf.json`: `minWidth: 960`, `minHeight: 620` の設定。

## 互換性（Compatibility）

- 既存のIPC通信・ジョブ実行ロジックへの影響なし。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの明示的フィードバックに基づく是正)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーのUI改善指示。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

# 仕様変更: テーブル横スクロール導入・カラム幅適正化および縦方向レスポンシブ伸長

Status: Applied

## Affected Specifications

- `docs/gui/app-shell.md`: Layout (Vertical responsive flex layout)
- `docs/gui/job-scheduling.md`: 1. ジョブ一覧テーブル（Column widths & Horizontal scroll）
- `apps/gui/src/components/JobTable.tsx`: カラム幅・横スクロール設定
- `apps/gui/src/App.tsx`: Content & Card の flex 構造
- `apps/gui/src/index.css`: `html, body, #root` の 100% 高さおよび Table flex スタイル

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - 横幅を問題なくフルに表示するために過大なウィンドウ幅が必要になり、「作業ディレクトリ」が圧縮されて「作.」と潰れる問題を解消し、横スクロールを導入する。
  - カラムヘッダーの「指示プロンプト」が2行に折り返されてしまう問題を解消する。
  - ウィンドウを縦に伸ばした際に下に余白ができる問題を解消し、縦方向にもレスポンシブに下まで伸びるようにする。
- **Agent Decision**:
  - **カラム幅と横スクロールの最適化**:
    - 「作業ディレクトリ」に `width: 260` を割り当て、中身が圧縮されて消失する問題を根本解消。
    - 「指示プロンプト」を `width: 140` に拡大し、かつヘッダー全体に `white-space: nowrap` を適用して改行を完全防止。
    - テーブルに `scroll={{ x: 1265 }}` を設定し、横幅が狭い場合でもスムーズに横スクロール可能とする。
    - 「操作」列は `fixed: "right"` を維持し、横スクロール時も常にボタン群にアクセス可能とする。
  - **縦方向レスポンシブ（Flex 100vh）**:
    - `html, body, #root` を `height: 100%` とし、`<Layout>`、`<Content>`、ジョブ一覧カードを `flex: 1; display: flex; flex-direction: column;` で連動。
    - テーブルコンテナおよび空データ（Empty）表示がカード中央・下端まで追従して広がり、下部に無駄なグレー余白が生じないようにする。

## 提案する差分（Proposed Delta）

- `apps/gui/src/components/JobTable.tsx`:
  - `columns`: 各カラム幅の適正化（作業ディレクトリ: 260, 指示プロンプト: 140）。
  - `scroll`: `{ x: 1265 }` へ更新。
- `apps/gui/src/App.tsx`:
  - `Layout`: `height: "100vh"`, `overflow: "hidden"`, `display: "flex"`, `flexDirection: "column"`.
  - `Content`: `flex: 1`, `display: "flex"`, `flexDirection: "column"`, `minHeight: 0`, `overflowY: "auto"`.
  - ジョブ一覧カード: `flex: 1`, `display: "flex"`, `flexDirection: "column"`.
- `apps/gui/src/index.css`:
  - `html, body, #root` の 100% 高さ指定。
  - `.ant-table-thead > tr > th` の `white-space: nowrap` 指定。
  - テーブルの flex 拡張スタイル。

## 互換性（Compatibility）

- 既存のデータモデルやAPIへの影響なし。UIレイアウトとUXの改善。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーからの明示的UI改善フィードバックに基づく)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーのUI指摘（作業ディレクトリの潰れ、指示プロンプトの改行、縦方向の余白）。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

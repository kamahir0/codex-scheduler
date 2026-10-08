# 仕様変更: ジョブ一覧テーブル内縦スクロール化およびページ表示件数カスタマイズ

Status: Applied

## Affected Specifications

- `docs/gui/job-scheduling.md`: 1. ジョブ一覧テーブル（Job Table）
- `apps/gui/src/components/JobTable.tsx`: テーブルスクロールおよびページネーションプロパティ
- `apps/gui/src/index.css`: テーブル内スクロールおよびフレックスレイアウト調整

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - 登録済みジョブ一覧テーブル内での縦スクロール（ヘッダー固定スクロール）を可能にする。
  - 1ページあたりの表示件数を変更可能にする（ページサイズ切り替え機能）。
- **Agent Decision**:
  - **テーブル内縦スクロール（Fixed Header & Vertical Scroll）**:
    - Ant Design `Table` の `scroll` 設定に `y` を追加（`scroll={{ x: 1285, y: 'calc(100vh - 350px)' }}`）。
    - ヘッダーがスクロール時も上部に固定され、カラムタイトルを見失わずに縦スクロールを可能にする。
    - ウィンドウリサイズ時も画面下部からはみ出さず、コンテナ内にスクロールが収まるようにする。
  - **ページネーションの拡張（Pagination & Page Size Customization）**:
    - 初期表示件数（デフォルト `pageSize`）を 10 件とする。
    - `showSizeChanger: true` を有効化し、`pageSizeOptions: ['10', '20', '50', '100']` を選択可能とする。
    - `showTotal` コールバックを導入し、総件数および現在表示中の件数範囲（例: `1-5 / 全 5 件`）を右下に明示する。
    - ユーザーが変更した `pageSize` はコンポーネントステートで保持され、操作中にリセットされないようにする。

## 提案する差分（Proposed Delta）

- `docs/gui/job-scheduling.md`:
  - ジョブ一覧テーブルの仕様に縦スクロール（`scroll.y`）およびページネーションのサイズ切替・総件数表示の要件を明記。
- `apps/gui/src/components/JobTable.tsx`:
  - `Table` コンポーネントに `scroll={{ x: 1285, y: 'calc(100vh - 350px)' }}` を設定。
  - `pagination` に `defaultPageSize: 10`, `showSizeChanger: true`, `pageSizeOptions: ['10', '20', '50', '100']`, `showTotal` を設定。
- `apps/gui/src/index.css`:
  - `.job-table-wrapper .ant-table-body` のスクロールバーのスタイルおよびフレックス追従の最適化。

## 互換性（Compatibility）

- 既存のデータモデル、API、スケジューラコアへの影響なし。UI/UXの機能拡張。

## レビュー（Review）

- **Blocking Issues**: None identified.
- **Non-blocking Issues**: None identified.
- **Questions**: None identified.
- **Approved as Proposed**: Yes.
- **Autonomous approval eligibility**:
  - Eligible: Yes
  - Human gate: None (ユーザーの直接UI指示に基づくフロントエンド改善)
  - Rationale: Human Decisionに基づく表示機能改善であり、データ永続化やOSスケジューラ等のコアセマンティクスに変更を与えないため。

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーのUI改善指示（「テーブル内縦スクロール化と1ページの表示件数変更を実装」）
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

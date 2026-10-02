# 仕様変更: ステータスバッジ行ズレ解消およびリトライ無効時の試行回数「–」表示

Status: Applied

## Affected Specifications

- `docs/gui/job-scheduling.md`: SCHED-UI-001 (Table columns)
- `apps/gui/src/components/JobTable.tsx`: getStatusBadge, attempts column render

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - テーブルの「待機中」等のステータスで、青丸ドットとタグテキストの行がズレて改行されていた不具合の修正。
  - リトライ設定が無効（オフ）のジョブについて、試行回数列に「0/6回」ではなく「–」と表示する。
- **Agent Decision**:
  - `getStatusBadge` 内で `<Badge status="..." />` と `<Tag />` を `display: inline-flex, alignItems: center, gap: 6, lineHeight: 1` で統合し、ドットとタグの垂直中央揃えを完全保証。
  - `record.retry_policy.enabled === false` の場合は試行回数を計算・表示せず、セカンダリ色のハイフン「–」を出力。

## 提案する差分（Proposed Delta）

- `apps/gui/src/components/JobTable.tsx`:
  - `getStatusBadge` の各ステータスで `span` ラッパーによる横一列整列。
  - 「ステータス」カラム幅を `185` に拡大し改行マージンを確保。
  - 「試行状況」カラムで `!record.retry_policy.enabled` のとき `<Text type="secondary">–</Text>` を返却。

## 互換性（Compatibility）

- 既存の内部モデルやAPIへの影響なし。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの明示的フィードバックに基づく是正)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーのUI不具合指摘。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

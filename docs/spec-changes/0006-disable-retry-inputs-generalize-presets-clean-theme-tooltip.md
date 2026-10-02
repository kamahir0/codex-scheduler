# 仕様変更: リトライ設定ディセーブル化・時刻プリセット汎用化・テーマTooltip衝突解消

Status: Applied

## Affected Specifications

- `docs/gui/job-scheduling.md`: SCHED-UI-002, SCHED-UI-003
- `apps/gui/src/components/CreateJobModal.tsx`: Form.useWatch, retry fields disabled, presets
- `apps/gui/src/App.tsx`: Theme Dropdown without Tooltip collision

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - リトライ設定がオフの時は、リトライ回数等の設定UIをディセーブル（無効化）にする。
  - 「深夜2時（推奨）」等の特定時刻への依存・決めつけを全廃し、相対時間（30分後、1時間後、2時間後、4時間後、明日同時刻）の汎用プリセットにする。
  - テーマ設定ボタンでホバー時に3択メニューと説明文Tooltipが重なって表示される問題を解消（Tooltipを削除）。
- **Agent Decision**:
  - `Form.useWatch` で `retry_enabled` のオン/オフをリアルタイム検知し、オフ時は `retry_interval_seconds` および `max_attempts` の `InputNumber` を `disabled` にし、コンテナの透明度を連動。
  - デフォルトの実行予定時刻を「1時間後」に設定。
  - ヘッダーのテーマ切り替えアイコンから Tooltip を除外し、Dropdown メニューのみがクリアにポップアップする構成に整理。

## 提案する差分（Proposed Delta）

- `apps/gui/src/components/CreateJobModal.tsx`:
  - `retry_interval_seconds` と `max_attempts` に `disabled={!retryEnabled}` を設定。
  - クイック指定を `30分後`, `1時間後`, `2時間後`, `4時間後`, `明日同時刻` に刷新。
  - 初期値を「1時間後」に変更。
- `apps/gui/src/App.tsx`:
  - テーマ Dropdown を包んでいた `<Tooltip>` を削除。

## 互換性（Compatibility）

- 既存の永続化モデルやIPC通信への影響なし。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザー指示に基づくUI洗練)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーのUI改善指示。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

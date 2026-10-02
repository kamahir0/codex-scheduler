# 仕様変更: 新規ジョブ登録時の初期実行日時の0分繰り上げ（分以下切り捨て＋1時間後）

Status: Applied

## Affected Specifications

- `docs/gui/job-scheduling.md`: 2. ジョブ登録モーダル（Scheduled Time 初期値）
- `apps/gui/src/components/CreateJobModal.tsx`: useEffect 内の defaultTarget 算出ロジック

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - 新規ジョブ登録モーダルを開いた際の「実行予定日時」の初期値を、現在時刻から分以下を切り捨てて0分にし、その1時間先の時刻とする。
- **Agent Decision**:
  - `dayjs().startOf("hour").add(1, "hour")` により、現在時刻が例えば `07:48` の場合は `08:00`、`14:05` の場合は `15:00` と直感的な毎時0分ジャストの時刻を初期値として設定する。

## 提案する差分（Proposed Delta）

- `apps/gui/src/components/CreateJobModal.tsx`:
  - `now.add(1, "hour").second(0)` から `now.startOf("hour").add(1, "hour")` へ更新。
- `docs/gui/job-scheduling.md`:
  - Scheduled Time の初期値規定を明記。

## 互換性（Compatibility）

- 既存の内部ジョブストアやAPIへの影響なし。フォーム入力初期値のUX改善。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの明示的指示に基づく)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーからの「新規ジョブ登録のときの時刻の初期値は、現在時刻から分以下を切り捨てて0分にし、その１時間先の時刻、にして」という指示。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

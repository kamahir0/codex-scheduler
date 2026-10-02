# 仕様: ジョブライフサイクルと永続化（Job Lifecycle and Persistence）

Status: Approved

Domain: SCHED

## 概要

`codex-scheduler` におけるジョブの定義、状態遷移、永続化、履歴管理の規範仕様を定める。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### SCHED-JOB-001: ジョブのデータ構造

ジョブ（Job）は以下の属性を MUST で保持しなければならない。

1. `id`: UUID v4 形式のユニーク文字列。
2. `provider`: 対象AIプロバイダ名（デフォルト `"codex"`）。
3. `session_id`: 対象AIサービスのセッション識別子（空文字は不可）。
4. `cwd`: 実行時のカレントワーキングディレクトリの絶対パス（ディレクトリが存在すること）。
5. `prompt`: 送信する指示テキスト（デフォルト `"continue"`）。
6. `scheduled_at`: 実行予定のISO 8601日時文字列（UTCまたはタイムゾーン付き）。
7. `status`: ジョブの現在状態（後述の `JobStatus`）。
8. `retry_policy`: リトライ設定（間隔、最大試行回数、タイムアウト）。
9. `created_at`: 作成日時。
10. `updated_at`: 最終更新日時。
11. `execution_history`: 各試行の記録（`ExecutionAttempt` のリスト）。

### SCHED-JOB-002: ジョブステータス遷移

ジョブの状態（`status`）は以下のいずれかでなければならず（MUST）、定義された遷移規則に従わなければならない。

- `scheduled`: 実行待ち状態。OSスケジューラに登録済み。
- `running`: workerによって現在実行中。
- `retrying`: 実行が一時失敗（Quota超過等）し、次回リトライ待ち。
- `succeeded`: プロンプト送信・実行が正常完了。
- `failed`: 最大リトライ回数超過、または致命的エラーにより失敗。
- `cancelled`: ユーザー操作により実行前に取り消された状態。

遷移規則:
- `scheduled` -> `running`
- `scheduled` -> `cancelled`
- `running` -> `succeeded`
- `running` -> `retrying` (リトライ条件を満たす場合)
- `running` -> `failed` (リトライ不可、または上限到達)
- `retrying` -> `running` (次回試行開始時)
- `retrying` -> `cancelled`

### SCHED-JOB-003: 永続化とアトミック性

1. ジョブストアはユーザーのホームディレクトリ配下（`~/.codex-scheduler/`）に保存しなければならない（MUST）。
2. ファイル書き込みは一時ファイル（`.tmp`）に書き込んでからアトミックにリネーム（replace）しなければならず、クラッシュ時のデータ破損を防止しなければならない（MUST）。
3. ジョブの追加、更新、削除は即座に永続化されなければならない（MUST）。

### SCHED-JOB-004: 実行履歴（ExecutionAttempt）の記録

各実行試行ごとに、以下の情報を含む `ExecutionAttempt` を履歴に追加しなければならない（MUST）。

- `attempt_number`: 試行番号（1から開始）。
- `started_at`: 実行開始日時。
- `finished_at`: 実行終了日時。
- `exit_code`: コマンド終了コード（プロセス異常終了時は null）。
- `stdout`: 標準出力（最大長制限あり）。
- `stderr`: 標準エラー出力。
- `is_quota_error`: Quota枯渇エラーと判定されたかどうかの真偽値。
- `error_message`: 失敗時の要約メッセージ。

### SCHED-JOB-005: ジョブのキャンセルと削除

1. ユーザーが `scheduled` または `retrying` のジョブをキャンセルした場合、ステータスを `cancelled` に更新し、OSスケジューラから該当タスクを登録解除しなければならない（MUST）。
2. ジョブが削除された場合、ストアから除去するとともに、OSスケジューラから該当タスクを確実に登録解除しなければならない（MUST）。

## 検証ルール

- `session_id` が空文字の場合は作成時にバリデーションエラーを返さなければならない。
- `cwd` で指定されたパスが存在しないディレクトリの場合はバリデーションエラーを返さなければならない。
- 過去の日時（現在時刻より1分以上前）が `scheduled_at` に指定された場合はバリデーションエラーを返さなければならない。

## 互換性

- 将来のプロバイダ追加やフィールド追加に対応するため、JSONシリアライズは未知のフィールドを許容する柔軟性を持たせる。

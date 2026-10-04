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
- `running`: worker/runnerによって現在実行中。
- `retrying`: 実行が一時失敗（Quota超過等）し、次回リトライ待ち。
- `succeeded`: プロンプト送信・実行が正常完了。
- `failed`: 最大リトライ回数超過、または致命的エラーにより失敗。
- `cancelled`: ユーザー操作により実行前に取り消された状態。

遷移規則:
- `scheduled` -> `running`
- `scheduled` -> `cancelled`
- `running` -> `succeeded`
- `running` -> `retrying` (リトライ条件を満たす場合、または回復可能な孤立検知時)
- `running` -> `failed` (リトライ不可、上限到達、または回復不能な孤立検知時)
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
- `stdout`: 標準出力（メモリおよび`jobs.json`の肥大化を防ぐため、最新の末尾 bounded size [最大10KB] を記録）。
- `stderr`: 標準エラー出力（最新の末尾 bounded size [最大10KB] を記録）。
- `is_quota_error`: Quota枯渇エラーと判定されたかどうかの真偽値。
- `error_message`: 失敗時の要約メッセージ。

完全なストリーミング実行ログは、ディスク上のログファイル（`~/.codex-scheduler/logs/<job_id>/attempt-<attempt_number>.log`）に保存されなければならない（MUST）。

### SCHED-JOB-005: ジョブのキャンセルと削除の排他保護

1. ユーザーが `scheduled` または `retrying` のジョブをキャンセルした場合、ステータスを `cancelled` に更新し、OSスケジューラから該当タスクを登録解除しなければならない（MUST）。
2. ステータスが `Running` であるジョブ、または RunnerLock や Codex 子プロセスが生存中（Liveness が `Dead` 以外）であるジョブに対して、キャンセル（`cancel`）および削除（`delete`）を試みた場合、操作を拒絶しエラーを返さなければならない（MUST NOT permit; MUST return error）。これにより、実行中ジョブのセッション排他 guard が消失して同一セッションへの second writer が起動されることを 100% 抑止しなければならない（MUST）。
3. ジョブの削除は、非実行中（`scheduled`, `retrying`, `succeeded`, `failed`, `cancelled` であり、かつ active な Runner または Codex 子プロセスが存在しない場合）にのみ許可され、ストアから除去するとともにログおよびメタデータをクリーンアップしなければならない（MUST）。

### SCHED-JOB-006: 長時間ジョブ実行ランナーと生存性（Liveness）保証

1. **Runner プロセス分離**:
   - ジョブの実行（Codex CLIの呼び出し、監視、ログ記録、結果確定）は、短時間のスケジューラtickプロセスとは分離された独立のRunnerプロセスによって行われなければならない（MUST）。
2. **OSレベル排他ロックと子プロセス実行メタデータの記録**:
   - Runnerプロセスは、ジョブ実行開始時にジョブ専用のロックファイル（`~/.codex-scheduler/runners/<job_id>.lock`）の排他ロック（exclusive file lock）を取得しなければならない（MUST）。
   - Runnerプロセスは、Codexの実行中、このファイルロックを解放せず保持し続けなければならない（MUST）。
   - Runnerプロセスは、Codex子プロセスの起動直後、ランナーメタデータ（`~/.codex-scheduler/runners/<job_id>.json`）に `runner_pid`、`codex_pid`、およびOSから取得した起動時刻（`codex_start_time`）を記録しなければならない（MUST）。
3. **三値 Liveness 判定とハンドオフ保護**:
   - Liveness 判定は、`Alive`（RunnerLock 保持または Codex 子プロセス生存）、`Dead`（OS 上でプロセスが終了していることを確認）、`Unknown`（システム呼出失敗・情報取得失敗等で生死を確定できない）の三値を明確に区別しなければならない（MUST）。
   - スケジューラ tick による claim から Runner プロセスがロックを取得するまでのハンドオフ期間中（起動猶予時間）、ジョブを孤立クラッシュと誤判定して回復してはならない（MUST NOT）。
   - `Unknown` の場合は fail-closed として扱い、ジョブを `Running` のまま維持して回復を保留し、同一セッションへの二重 writer 起動をブロックしなければならない（MUST）。

### SCHED-JOB-007: クラッシュおよび孤立ジョブの自動回復（Orphan Recovery）

1. **孤立Runningジョブの検知とCodex子プロセス保護**:
   - スケジューラtick実行時、ストア上でステータスが `Running` であるジョブについて、対応するロックファイルの排他ロック取得を試行しなければならない（MUST）。
   - ロックが取得できない場合（`WouldBlock` 等）、Runnerプロセスは現在正常に生存・実行中であると判定し、ステータスを変更してはならない（MUST NOT）。
   - ロックが取得できた場合（Runnerプロセスが失われた場合）、記録された `codex_pid` および `codex_start_time` に基づいてCodex子プロセスの Liveness を照合しなければならない（MUST）。
   - **Codex子プロセスが `Alive` または `Unknown` の場合、ステータスを `Running` のまま維持し、回復処理を行ってはならない（MUST NOT）**。これにより同一セッションに対する二重writer起動を防止しなければならない（MUST）。
2. **回復アクション**:
   - ハンドオフ猶予期間を過ぎ、かつ Codex 子プロセスが確実に `Dead` であることが確認された場合にのみ、異常終了試行（`exit_code: None`, `error_message: "Runner process terminated unexpectedly (process crash or system restart)"`）を追加し、リトライポリシーに基づいて `Failed` または `Retrying` へ安全に遷移させ、メタデータとロックファイルをクリーンアップしなければならない（MUST）。これによりジョブが永久に `Running` に取り残されることを防止する。

## 検証ルール

- `session_id` が空文字の場合は作成時にバリデーションエラーを返さなければならない。
- `cwd` で指定されたパスが存在しないディレクトリの場合はバリデーションエラーを返さなければならない。
- 過去の日時（現在時刻より1分以上前）が `scheduled_at` に指定された場合はバリデーションエラーを返さなければならない。

## 互換性

- 将来のプロバイダ追加やフィールド追加に対応するため、JSONシリアライズは未知のフィールドを許容する柔軟性を持たせる。

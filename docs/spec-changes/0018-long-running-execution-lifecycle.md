# 仕様変更: 長時間Codex実行ライフサイクルの分離と堅牢化（Specification change: Long-running Codex Execution Lifecycle）

Status: Applied

## Affected Specifications

- `docs/specs/job-lifecycle.md` (改訂 `SCHED-JOB-002`, `SCHED-JOB-004`, 新規 `SCHED-JOB-006`, 新規 `SCHED-JOB-007`)
- `docs/specs/os-scheduler.md` (改訂 `OS-SCHED-003`)
- `docs/specs/codex-adapter.md` (新規 `CODEX-RESUME-009`)
- `docs/specs/cli.md` (改訂 `CLI-CMD-002`)

## 根拠と分類（Source Evidence and Classification）

- **Requirement (Human Intent)**:
  - Schedulerによってquota解除後などにCodex sessionを`continue`で再開した際、Codexの実作業が数十分〜数時間継続しても、OS scheduler tickを長時間占有せず、jobを正しくRunningとして管理し、実際のCodex execution終了時にだけSucceeded / Failed / Retryingを確定できる実行ライフサイクルを実現する。
  - tickはjob executionを安全に引き渡した後、短時間で終了可能であること。
  - 正常起動されたlong-running process実行中はRunningを維持し、stdout/stderrの未完結や通常progress出力をfailureとしない。
  - 1つのlong-running jobによって後続のscheduler tickや別due jobの処理が恒常的に妨げられないようにし、同一jobの二重実行を防止する。
  - runnerプロセスの異常終了・マシン再起動時にjobが永久にRunningに残らないようにしつつ、生存している長時間プロセスをstaleと誤判定して二重実行しない。
  - stdout/stderrをメモリに無制限蓄積せず、jobs.jsonを肥大化させない安全なログ保持とし、実行中状況をread-onlyで確認可能にする。
  - monitoring目的での追加resumeや二重writer起動を防止し、active writer競合を適切に扱う。
  - Desktop / CLI共通で`codex-scheduler-core`の同一execution semanticsを使用し、macOSとWindowsで同等のライフサイクルを提供する。
  - 既存`jobs.json`のbackward-compatibleな読み込みと既存CLI互換性を維持する。
- **Constraint (Human Intent)**:
  - PIDだけに依存したunsafeなliveness判定や、固定の「5分以上Runningならstale」のように正常な長時間taskを誤kill/再実行する方式は避ける。
  - 実際に数時間待つtestに依存せず、controllable fake/helper processを用いて短時間・決定論的に検証する。
  - breaking persisted-format migrationは避ける。
  - 実行中jobプロセスの強制終了（cancel semantics拡張）は明示的非スコープ。
- **Agent Decision**:
  - **Runner Process分離**: tickプロセスはdue jobをclaim後、独立したheadless runnerプロセス（CLI環境: `codex-scheduler run-job <job_id>`、Desktop環境: `codex-scheduler-gui --run-job <job_id>`）を非同期・detachedで起動し、数ミリ秒〜数十ミリ秒で正常終了する。
  - **OSファイルロックに基づくLiveness検証**: runnerプロセスはジョブ専用のロックファイル（`~/.codex-scheduler/runners/<job_id>.lock`）の排他ロック（`fs2::FileExt::try_lock_exclusive`）を起動から終了まで保持する。プロセス終了時はOSカーネルが自動的にファイルロックを解放するため、後続tickやreconcile処理は非ブロッキングで「ロック保持中＝生存中」「ロック取得可能＝異常終了/クラッシュ」と100%確実に判定できる（PID wrap-aroundの誤認なし）。
  - **Orphan Runningの自動回復**: tick実行時、ストア上で`Running`状態だがロックファイルが保持されていないジョブを検出した場合、クラッシュまたは再起動により死亡したorphanジョブと判定し、失敗試行を記録して`Failed`（またはリトライ可能なら`Retrying`）へ安全に復帰させる。
  - **ストリーミングファイルログとBoundedメモリ保持**: runnerはCodex実行のstdout/stderrを専用ログファイル（`~/.codex-scheduler/logs/<job_id>/attempt-<N>.log`）へストリーミング追記する。`ExecutionAttempt`には末尾一定サイズ（最大10KB / 100行）のbounded tailのみを保持し、`jobs.json`の肥大化とメモリ枯渇を防ぐ。
  - **同一Sessionの並行実行抑止**: tickにおいて、同一`session_id`を持つジョブが既に`Running`状態である場合、別のジョブのclaimを次回以降のtickへ延期し、同一sessionへの二重writer競合をScheduler側で未然に防止する。

## 提案する差分（Proposed Delta）

### 1. `docs/specs/job-lifecycle.md`

#### SCHED-JOB-002: ジョブステータス遷移（改訂）
遷移規則にOrphan Recoveryの遷移を追加する。
- `running` -> `failed` (リトライ不可、上限到達、またはrunnerクラッシュによる孤立検知時)
- `running` -> `retrying` (リトライ条件を満たす一時失敗、または回復可能な孤立検知時)

#### SCHED-JOB-004: 実行履歴（ExecutionAttempt）の記録（改訂）
- `stdout` / `stderr`: メモリおよび`jobs.json`の無制限膨張を防ぐため、`ExecutionAttempt`に保持する文字列は最新の末尾 bounded size（最大10KB）に切り詰めて記録しなければならない（MUST）。
- 完全な全量出力はジョブ実行ログファイル（`~/.codex-scheduler/logs/<job_id>/attempt-<attempt_number>.log`）にストリーミング保存されなければならない（MUST）。

#### SCHED-JOB-006: 長時間ジョブ実行ランナーと生存性（Liveness）保証（新規）
1. **Runner プロセス分離**:
   - ジョブの実行（Codex CLIの呼び出し、監視、ログ記録、結果確定）は、短時間のスケジューラtickプロセスとは分離された独立のRunnerプロセスによって行われなければならない（MUST）。
2. **OSレベル排他ロックによるLiveness管理**:
   - Runnerプロセスは、ジョブ実行開始時にジョブ専用のロックファイル（`~/.codex-scheduler/runners/<job_id>.lock`）の排他ロック（exclusive file lock）を取得しなければならない（MUST）。
   - Runnerプロセスは、Codexの実行中、このファイルロックを解放せず保持し続けなければならない（MUST）。
   - プロセスの正常終了、異常終了、シグナル停止、マシン再起動のいずれが発生した場合でも、OSカーネルによって当該ロックが自動解放されることを利用し、Liveness判定の唯一の厳密な根拠としなければならない（MUST）。PIDの存在確認のみに依存した判定を行ってはならない（MUST NOT）。

#### SCHED-JOB-007: クラッシュおよび孤立ジョブの自動回復（Orphan Recovery）（新規）
1. **孤立Runningジョブの検知**:
   - スケジューラtick実行時、ストア上でステータスが `Running` であるジョブについて、対応するロックファイルの排他ロック取得を試行しなければならない（MUST）。
   - ロックが取得できない場合（`WouldBlock` 等）、Runnerプロセスは現在正常に生存・実行中であると判定し、ステータスを変更してはならない（MUST NOT）。
   - ロックが取得できた場合、当該ジョブを実行していたRunnerプロセスは予期せず終了（クラッシュ、電源断等）した孤立（orphan）状態であると判定しなければならない（MUST）。
2. **回復アクション**:
   - 孤立ジョブを検知した場合、異常終了を示す `ExecutionAttempt`（`exit_code: None`, `error_message: "Runner process terminated unexpectedly (process crash or system restart)"`）を追加し、リトライポリシーに基づいて `Failed` または `Retrying` へ安全に遷移させなければならない（MUST）。これによりジョブが永久に `Running` に取り残されることを防止する。

---

### 2. `docs/specs/os-scheduler.md`

#### OS-SCHED-003: ヘッドレスモード要件と共有コアセマンティクス（改訂）
1. **ヘッドレスモード要件（`--scheduler-tick`）**:
   - LaunchAgent または Task Scheduler から起動された tick プロセスは、期限到来したジョブをアトミックに抽出（claim）し、各ジョブに対応する Runner プロセスを安全にバックグラウンド起動（detached spawn）した後、**数ミリ秒〜数十ミリ秒以内に直ちに正常終了（exit code 0）しなければならない（MUST）**。
   - 長時間継続する Codex 実行プロセスの完了を tick プロセス自身が同期的に待機（await）してはならない（MUST NOT）。
2. **多重実行防止（Atomic Claim）と同一Session排他**:
   - 同一ジョブが二重実行されてはならない（MUST NOT）。
   - 同一の `session_id` を持つ別のジョブが現在 `Running` 状態である場合、後続の同一セッション向けジョブの claim は実行中のジョブが完了するまで保留（スキップ）し、同一セッションに対する二重writer競合を防止しなければならない（MUST）。
3. **共有コアセマンティクス**:
   - Desktop所有時（`codex-scheduler-gui --run-job <job_id>`）およびCLI所有時（`codex-scheduler run-job <job_id>`）のいずれにおいても、Runnerプロセスは同一の `codex-scheduler-core` の実行ロジックを使用しなければならない（MUST）。

---

### 3. `docs/specs/codex-adapter.md`

#### CODEX-RESUME-009: ストリーミング出力、安全なログ保持、およびRead-only可観測性（新規）
1. **ストリーミングログファイル保存**:
   - Codexプロセスの標準出力（stdout）および標準エラー出力（stderr）は、メモリバッファへ無制限に溜め込んではならず（MUST NOT）、ジョブ試行ごとのログファイル（`~/.codex-scheduler/logs/<job_id>/attempt-<attempt_number>.log`）へストリーミングで書き込まなければならない（MUST）。
2. **Read-only進捗確認**:
   - 実行中のジョブの進捗状況は、当該ログファイルを読み取る（read-only）ことで外部ツールやCLIから安全に確認できなければならない（MUST）。
   - 実行中の状況を確認する目的で、同一Codexセッションに対して追加の `resume` コマンドを発行してはならない（MUST NOT）。
3. **Quotaエラー判定**:
   - プロセス終了時、出力ログの全体（またはストリーミング中に検出したシグネチャ）に基づいて `is_quota_error` を判定しなければならない（MUST）。

---

### 4. `docs/specs/cli.md`

#### CLI-CMD-002: 正式コマンドサーフェス（改訂）
- サブコマンド `run-job <job_id>`:
  - 指定されたジョブIDを独立Runnerプロセスとして実行する。
  - OSスケジューラのtickプロセスからdetachedに起動される内部用途、および手動デバッグ実行の双方で共有される。
  - Desktop環境向けにも同一引数（`--run-job <job_id>`）がサポートされる。
- サブコマンド `show <job_id>`:
  - 実行中ジョブについて、現在保持されているログファイルのパスおよび直近の出力を表示できる（read-only）。

## 互換性（Compatibility）

- **`jobs.json` スキーマ互換性**: 既存の `Job` / `ExecutionAttempt` 構造体のフィールド（`stdout`, `stderr` 等）を削除・変更せず、完全な上位互換性を維持する。
- **CLI コマンド互換性**: 既存の `schedule`, `list`, `show`, `cancel`, `delete`, `status`, `tick` の引数・JSON出力スキーマを変更しない。
- **OS スケジューラ互換性**: macOS `dev.codexscheduler.scheduler`（LaunchAgent）および Windows `CodexScheduler_Service`（Task Scheduler）の登録内容・引数（`--scheduler-tick`）を変更せず、そのまま利用可能。

## 受け入れと実装への影響（Acceptance and Implementation Impact）

- **検証受入条件**:
  - fake child processを用いたテストにより、tickが長時間実行子の完了を待たず即座に正常復帰することを検証。
  - tick復帰後もジョブが`Running`のまま子プロセスが継続して動作することを検証。
  - 後続のtickが正常に動作し、別のdue jobを並行処理できることを検証。
  - 同一jobが二重起動されないことを検証。
  - 同一session_idを持つ別jobが先行実行中の場合、先行ジョブ完了までclaimが保留されることを検証。
  - runnerプロセスがクラッシュ（またはロックファイル解放）した場合、後続tickによって安全にorphan検知され、永久Runningにならず`Failed`/`Retrying`に回復することを検証。
  - ログファイルへのストリーミング記録と、`ExecutionAttempt`のbounded tail保持により、大量出力でもメモリ・`jobs.json`が肥大化しないことを検証。
  - macOSおよびWindows環境の双方でテストが成功すること。

## 未解決事項（Open Questions）

None. (すべての論点がAgent Decisionとして整合的に解決されている)

## レビュー（Review）

- **Blocking Issues**: None identified
- **Non-blocking Issues**: None identified
- **Questions**: None identified
- **Approved as Proposed**: Yes
- **Autonomous approval eligibility**:
  - Eligible: Yes
  - Human gate: None (Human明示選択済みのCurrent Objective範囲内、non-breaking、persisted format後方互換、CLI公開仕様互換)
  - Rationale: 本仕様変更は、問題提起された長時間実行時のtick専有・永久Running・ログ肥大化を解決するための最小かつ自然なアーキテクチャ拡張であり、既存の永続化形式やCLI契約を一切破壊しないため。

## 承認記録（Approval Record）

- **Approval mode**: Agent-autonomous
- **Basis**: Current Objective "Long-running Codex execution lifecycle" authorized by Human
- **Review result**: All criteria met, no blocking findings, Human gate: None.

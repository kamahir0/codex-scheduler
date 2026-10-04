# 仕様変更: 長時間実行ライフサイクルの堅牢化と観測性・ログ安全性の向上（Specification change: Long-running Execution Lifecycle Corrections）

Status: Applied

## レビュー（Review）

- **Blocking Issues**: None identified
- **Non-blocking Issues**: None identified
- **Questions**: None identified
- **Approved as Proposed**: Yes
- **Autonomous approval eligibility**:
  - Eligible: Yes
  - Human gate: None (Current Objective範囲内、後方互換維持、非破壊的安全性設計)
- **Approval Record**: Agent-autonomous approved on 2026-10-05

## Affected Specifications

- `docs/specs/job-lifecycle.md` (改訂 `SCHED-JOB-006`, `SCHED-JOB-007`)
- `docs/specs/codex-adapter.md` (改訂 `CODEX-RESUME-009`)
- `docs/specs/cli.md` (改訂 `CLI-CMD-002`, `CLI-CMD-003`)

## 根拠と分類（Source Evidence and Classification）

- **Requirement (Human Intent)**:
  - 生存している長時間processをstaleと誤判定して二重実行しない。
  - liveness proofはRunner processだけでなく、実際のCodex execution lifetimeを安全に表現すること。
  - Runner crash後にCodex childが生存している場合、別writerを起動してはならない。
  - PID existenceだけを唯一の根拠にしてはならない（PID wrap-around対策）。
  - fixed timeoutでstale判定してはならない。
  - macOS / Windows両方で意味上同じ安全性を持たせる。
  - reboot recoveryも維持する。
  - destructive kill policyが必要ならHuman gate判定を行う（今回は非破壊的child生存追跡を採用しHuman gate回避）。
  - stdout/stderr streaming中にquota signatureをincrementalに検出・保持し、最終10KB tailにsignatureが残っていなくても`is_quota_error == true`と判定できること。
  - `show <job-id> --json`でも、Running時にmachine-readableなactive execution observability（log path, bounded latest output, runner active state）を取得可能にすること。
  - ログの寿命・ディスク使用量上限・ローテーション・削除・オープン失敗時のセマンティクスを明示的に規定すること。
- **Constraint (Human Intent)**:
  - 破壊的な強制終了（destructive kill policy）は本Objectiveでは導入せず、Codex child生存中はRunningを維持して二重writerを抑止する。
  - 単一valid JSON契約およびCLI/jobs.jsonの後方互換性を破壊しない。
  - release/tag/publishは行わない。
- **Agent Decision**:
  - **二層Liveness検証とRunnerInfoメタデータ**:
    Runnerプロセス起動時に `~/.codex-scheduler/runners/<job_id>.json`（`RunnerInfo`）を作成し、`runner_pid`, `runner_started_at`, `codex_pid`, `codex_start_time` を永続化する。
    1. **Primary Liveness**: Runnerプロセスが保持するOS排他ファイルロック（`RunnerLock`）。ロック保持中は正常実行中。
    2. **Child Liveness (Orphan Guard)**: Runner lockが喪失した場合でも、記録された `codex_pid` および `codex_start_time`（Windows: `GetProcessTimes` による CreationTime、macOS/Unix: `ps -o lstart=` による起動時刻）を照合し、子プロセスがOS上で生存しているかを確認する。
       - 子プロセスが生存している場合: ジョブを `Running` のまま維持し、同一 `session_id` に対する新規ジョブのclaimをブロックし続ける（二重writer競合を100%防止）。
       - 子プロセスが終了している（またはマシン再起動により存在しない）場合: 完全に実行終了したと判定し、試行失敗を記録して `Failed`/`Retrying` へ安全に自動回復し、メタデータとロックをクリーンアップする。
  - **Streaming Quota Signature Incremental Detection**:
    `execute_resume_streaming` において、ストリーミングチャンクの受信時に逐次（incremental）にquota signatureを判定・保持するフラグ（`AtomicBool` またはタスク内ステート）を管理する。10KB tailで切り詰められる前の初期出力にクォータ枯渇メッセージが含まれていた場合でも、プロセス終了時に `is_quota_error = true` を確実に確定する。
  - **Log Safety Policy**:
    1. **Attemptログ上限**: 1回の試行ログ（`attempt-<N>.log`）の最大ファイルサイズを10MB（`MAX_LOG_FILE_BYTES`）に制限。上限到達時は truncation メッセージを記録し、それ以上のファイル膨張を停止。
    2. **Terminal Job Cleanup**: ジョブ削除時（`delete <job-id>`）、該当ジョブのログディレクトリ（`~/.codex-scheduler/logs/<job_id>`）を完全に削除。
    3. **Log Open/Write Failure Semantics**: ログファイルの作成・追記に失敗した場合でも、Codex executionそのものは中断・失敗させず、メモリ内 bounded tail で継続実行する（ログファイルはベストエフォート可観測性サポート）。
  - **Machine-readable JSON Observability**:
    `show <job-id> --json` において、ジョブステータスが `Running` の場合、トップレベルに `active_execution` フィールド（`is_runner_active`, `attempt_number`, `log_path`, `latest_output`）を additive に付与する。

## 提案する差分（Proposed Delta）

### 1. `docs/specs/job-lifecycle.md`

#### SCHED-JOB-006: 長時間ジョブ実行ランナーと生存性（Liveness）保証（改訂）
- Runnerプロセスは、Codex子プロセスの起動直後、ランナーメタデータ（`~/.codex-scheduler/runners/<job_id>.json`）に `runner_pid`、`codex_pid`、およびOSから取得した起動時刻（`codex_start_time`）を記録しなければならない（MUST）。
- Liveness判定は、Runnerプロセスが保持するファイルロック（`RunnerLock`）を第一根拠とし、ロックが失われた場合でもCodex子プロセスのOS生存確認（PIDおよび起動時刻の照合）を行わなければならない（MUST）。PID存在確認のみに依存した判定を行ってはならない（MUST NOT）。

#### SCHED-JOB-007: クラッシュおよび孤立ジョブの自動回復（Orphan Recovery）（改訂）
- スケジューラtick実行時、ステータスが `Running` でRunnerロックが取得できた（Runnerプロセスが失われた）ジョブについて、記録された `codex_pid` および `codex_start_time` に基づいてCodex子プロセスの生存を確認しなければならない（MUST）。
- Codex子プロセスがOS上で生存している場合、ステータスを `Running` のまま維持し、回復処理を行ってはならない（MUST NOT）。これにより同一セッションに対する二重writer起動を防止しなければならない（MUST）。
- Codex子プロセスが終了している（または再起動等により存在しない）ことが確認された場合にのみ、異常終了試行を追加して `Failed` または `Retrying` へ遷移させ、メタデータとロックファイルをクリーンアップしなければならない（MUST）。

---

### 2. `docs/specs/codex-adapter.md`

#### CODEX-RESUME-009: ストリーミング出力、安全なログ保持、およびRead-only可観測性（改訂）
- **ストリーミングIncremental Quota検知**:
  - stdoutおよびstderrのストリーミング中、チャンク受信ごとにquota signatureの有無を逐次検知・保持しなければならない（MUST）。
  - プロセス終了時の判定は、メモリ内の最終bounded tailだけでなく、ストリーミング中に検知されたsignatureフラグを包含しなければならず（MUST）、初期に出力されたquotaエラーがtailから切り捨てられた場合でも `is_quota_error == true` としなければならない（MUST）。
- **ログ安全性ポリシー**:
  - 1試行あたりのログファイルサイズ上限は10MBとし、これを超える追記は安全に停止（キャップ）しなければならない（MUST）。
  - ログファイルのオープンまたは書き込みに失敗した場合でも、Codex実行そのものは中断してはならず（MUST NOT）、インメモリ処理で継続しなければならない（MUST）。
  - ジョブが削除された場合、対応するログファイルディレクトリ（`~/.codex-scheduler/logs/<job_id>`）を確実に削除しなければならない（MUST）。

---

### 3. `docs/specs/cli.md`

#### CLI-CMD-002: 正式コマンドサーフェス（改訂）
- `show <job-id> --json`:
  - ジョブステータスが `Running` の場合、JSON出力に `active_execution` オブジェクトを含めなければならない（MUST）。
  - `active_execution` は `is_runner_active` (bool)、`attempt_number` (u32)、`log_path` (string)、`latest_output` (string) を含み、単一の有効なJSONとして標準出力に出力されなければならない（MUST）。

## 互換性（Compatibility Impact）

- `jobs.json` のスキーマは変更せず、後方互換性を完全維持。
- CLIのJSON出力は既存フィールドを変更せず、`active_execution` をオプショナルに追加する additive な変更であるため完全な後方互換性を維持。
- 破壊的な外部変更なし。

## 実装への影響（Implementation Impact）

- `crates/codex-scheduler-core/src/runner.rs`: `RunnerInfo` 構造体、OSプロセス起動時刻取得（Windows / Unix）、子プロセスのLiveness照合、10MBログ上限ガードの実装。
- `crates/codex-scheduler-core/src/adapter/codex.rs`: ストリーミング中 incremental quota 検知、ログオープン失敗時のフォールバック。
- `crates/codex-scheduler-core/src/lib.rs`: `reconcile_running_jobs` での Codex child 生存確認ガード。
- `crates/codex-scheduler-cli/src/main.rs`: `show --json` での `active_execution` 出力。
- `skills/scheduler-cli/SKILL.md`: `show --json` の `active_execution` 確認フローの明記。

## 承認適格性（Approval Eligibility）

- **Autonomous approval eligible**: Yes
- **Human gate**: None (Human明示選択済みのCurrent Objective範囲内、non-breaking、破壊的kill policyを行わない非破壊的安全性設計)

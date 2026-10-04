# 仕様変更: 長時間実行ライフサイクルのプロセス排他・生存性判定・ログ上限の厳格化（Specification change: Execution Lifecycle Hardening）

Status: Applied

## レビュー（Review）

- **Blocking Issues**: None identified
- **Non-blocking Issues**: None identified
- **Questions**: None identified
- **Approved as Proposed**: Yes
- **Autonomous approval eligibility**:
  - Eligible: Yes
  - Human gate: None (Current Objective範囲内の排他安全性向上、fail-closed回復、厳格なログ上限適用、および後方互換な観測性追加)
- **Approval Record**: Agent-autonomous approved on 2026-10-05

## Affected Specifications

- `docs/specs/job-lifecycle.md` (改訂 `SCHED-JOB-005`, `SCHED-JOB-006`, `SCHED-JOB-007`)
- `docs/specs/os-scheduler.md` (改訂 `OS-SCHED-003`)
- `docs/specs/codex-adapter.md` (改訂 `CODEX-RESUME-009`)
- `docs/specs/cli.md` (改訂 `CLI-CMD-002`, `CLI-CMD-003`)

## 根拠と分類（Source Evidence and Classification）

- **Requirement (Human Intent)**:
  - Running ジョブに対する cancel / delete 操作によって、生存中の Codex 子プロセスに対するセッション排他ガード（single-writer guard）が消失して二重 writer が起動することを 100% 抑止すること。
  - 同一セッションに対する single-writer invariant を、tick だけでなく GUI run-now、手動 `execute_job`、CLI `run-job` などのすべての実行エントリポイントでアトミックに強制すること。
  - claim 直後から RunnerLock 取得までのハンドオフ期間中に発生し得る誤った孤立判定（premature orphan recovery）を防止すること。
  - プロセス生存確認（Liveness）において、観測失敗を安易に死亡（Dead）とみなさず、`Alive` / `Dead` / `Unknown` の三値を区別し、`Unknown` 時は fail-closed としてジョブを回復せず二重 writer を起動しないこと。
  - ディスクログファイルサイズの上限（10MB）を、stdout/stderr 合算かつロック保持下で厳格に計算し、10MB を 1 バイトも超過しないこと。
  - AI エージェント向けに、`show --json` の `active_execution` において Runner プロセスだけでなく Codex 子プロセスの生存状況（`codex_process_alive`, `liveness_state`）を機械可読に出力すること。
- **Constraint (Human Intent)**:
  - 実行中プロセスの強制終了（SIGKILL等）は explicit non-scope を維持し、実行中のジョブに対する変更を拒絶することで安全性を担保する。
  - 単一 valid JSON 出力契約および既存フィールドの後方互換性を破壊しない。
  - release / tag / publish は行わない。
- **Agent Decision**:
  - **Running Job Mutation Guard (`SCHED-JOB-005`)**:
    ステータスが `Running`、または RunnerLock / Codex 子プロセスの Liveness が `Dead` 以外であるジョブに対する `cancel_job` および `delete_job` はエラー（`CannotCancelRunningJob` / `CannotDeleteRunningJob`）として拒絶する。
  - **Universal Same-Session Single-Writer Guard (`OS-SCHED-003`, `SCHED-JOB-006`)**:
    `claim_job_for_execution` は、対象ジョブ自身のステータス検査に加え、同一 `session_id` を持つ他のジョブが `Running` であるか、または active な実行（Runner または Codex 子プロセスが生存中）を保持しているかをアトミックに検査し、競合時は `SessionBusy` エラーで拒絶する。
    `run_job_runner` は、RunnerLock 取得時であっても、記録された前回の Codex 子プロセスが生存している場合は second writer の起動を阻止する。
  - **Handoff Grace Period & Three-state Liveness (`SCHED-JOB-006`, `SCHED-JOB-007`)**:
    1. Claim 直後から Runner プロセスがロックを取得するまでの間、一定の猶予時間（Grace Period）内は孤立回復を保留する。
    2. Liveness 判定を `Alive(Runner | Child)`, `Dead`, `Unknown` に分類し、OS API の一時的失敗や情報取得失敗時は `Unknown` と判定。`Unknown` の場合はジョブを `Running` のまま維持し、回復処理や新規 writer 起動をブロックする。
  - **Strict 10MB Capped Log Writer (`CODEX-RESUME-009`)**:
    共有の `CappedLogWriter` を導入し、ファイルロック（Mutex）を取得した状態で実残り容量を計算。合計ファイルサイズが `MAX_LOG_FILE_BYTES`（10MB = 10,485,760 バイト）を厳密に超えないよう書き込み長を制御する。
  - **Additive Machine-readable Liveness Schema (`CLI-CMD-003`)**:
    `show --json` の `active_execution` に `codex_process_alive: bool` および `liveness_state: "runner_active" | "child_active" | "unknown"` を付与する。

## 提案する差分（Proposed Delta）

### 1. `docs/specs/job-lifecycle.md`

#### SCHED-JOB-005: ジョブのキャンセルと削除の排他保護（改訂）
- ステータスが `Running` であるジョブ、または RunnerLock や Codex 子プロセスが生存（Liveness が `Dead` 以外）しているジョブに対して、キャンセル（`cancel`）および削除（`delete`）を試みた場合、操作を拒絶しエラーを返さなければならない（MUST NOT permit; MUST return error）。
- ジョブのキャンセルは `scheduled` または `retrying` のジョブにのみ適用可能とする（MUST）。
- ジョブの削除は、非実行中（`scheduled`, `retrying`, `succeeded`, `failed`, `cancelled` であり、かつ active な Runner または Codex 子プロセスが存在しない場合）にのみ適用可能とする（MUST）。

#### SCHED-JOB-006: 長時間ジョブ実行ランナーと三値生存性（Liveness）保証（改訂）
- スケジューラ tick による claim から Runner プロセスがロックを取得するまでのハンドオフ期間中、ジョブを孤立クラッシュと誤判定して回復してはならない（MUST NOT）。
- Liveness 判定は、`Alive`（RunnerLock 保持または Codex 子プロセス生存）、`Dead`（OS 上でプロセスが終了していることを確認）、`Unknown`（システム呼出失敗・情報取得失敗等で生死を確定できない）の三値を明確に区別しなければならない（MUST）。
- `Unknown` の場合は fail-closed として扱い、ジョブを `Running` のまま維持して回復を保留し、同一セッションへの二重 writer 起動をブロックしなければならない（MUST）。

#### SCHED-JOB-007: クラッシュおよび孤立ジョブの自動回復（Orphan Recovery）（改訂）
- スケジューラ tick における孤立回復処理は、Runner ロックが解放されており、かつ Codex 子プロセスが確実に `Dead` であることが判定された場合にのみ実行されなければならない（MUST）。`Unknown` 状態のジョブを回復してはならない（MUST NOT）。

---

### 2. `docs/specs/os-scheduler.md`

#### OS-SCHED-003: 同一セッション単一ライター保護の全エントリポイント強制（改訂）
- スケジューラ tick による一括クレーム（`claim_due_jobs`）だけでなく、手動即時実行（GUI run-now、CLI run-job、`execute_job`）を含むすべての実行エントリポイントにおいて、同一 `session_id` を持つジョブのアクティブ実行（`Running` または `liveness_state != Dead`）が存在する場合、新規実行のクレームをアトミックに拒絶しなければならない（MUST）。

---

### 3. `docs/specs/codex-adapter.md`

#### CODEX-RESUME-009: ストリーミング出力、安全なログ保持、およびRead-only可観測性（改訂）
- ディスクログファイルサイズは、stdout / stderr 合算で `MAX_LOG_FILE_BYTES`（10MB = 10,485,760 バイト）を厳密に超えてはならない（MUST NOT）。
- 残り書き込み容量はファイル排他ロック下で逐次計算されなければならず、上限を超える出力は切り捨てられなければならない（MUST）。

---

### 4. `docs/specs/cli.md`

#### CLI-CMD-003: 機械可読 JSON 出力モード（改訂）
- `show <job-id> --json`:
  - `active_execution` オブジェクトに `codex_process_alive: bool` および `liveness_state: "runner_active" | "child_active" | "unknown"` を含めなければならない（MUST）。

## 互換性（Compatibility Impact）

- `jobs.json` の永続化スキーマは完全維持。
- CLI の `show --json` は既存プロパティを破壊せず additive に拡張するため完全な後方互換性を維持。
- 実行中ジョブに対する `cancel` / `delete` の拒絶は、未定義の異常動作や二重実行事故を防止する健全化であり、既存の正規なキャンセル仕様（`scheduled`/`retrying`対象）と完全一致。

## 承認適格性（Approval Eligibility）

- **Autonomous approval eligible**: Yes
- **Human gate**: None (Human 指摘の Blocking 解消、Current Objective 範囲内、非破壊的安全設計)

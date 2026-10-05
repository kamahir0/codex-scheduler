# 仕様変更: 実行ライフサイクルの耐久的エビデンス・自己ロック分離・メタデータ整合性・ログ上限の厳格化（Specification change: Durable Execution Lifecycle Hardening）

Status: Applied

## レビュー（Review）

- **Blocking Issues**: None identified
- **Non-blocking Issues**: None identified
- **Questions**: None identified
- **Approved as Proposed**: Yes
- **Autonomous approval eligibility**:
  - Eligible: Yes
  - Human gate: None (Current Objective範囲内の排他安全性向上、客観的耐久エビデンスに基づく孤立判定、フェイルクローズなメタデータ破損対応、既存ログに対する厳格な上限適用、および可観測性の整合性確保)
- **Approval Record**: Agent-autonomous approved on 2026-10-05

## Affected Specifications

- `docs/specs/job-lifecycle.md` (改訂 `SCHED-JOB-006`, `SCHED-JOB-007`)
- `docs/specs/os-scheduler.md` (改訂 `OS-SCHED-003`)
- `docs/specs/codex-adapter.md` (改訂 `CODEX-RESUME-009`)
- `docs/specs/cli.md` (改訂 `CLI-CMD-002`, `CLI-CMD-003`)

## 根拠と分類（Source Evidence and Classification）

- **Requirement (Human Intent)**:
  - **Manual `run-job` self-lock regression 解消**:
    `codex-scheduler run-job <id>` を Scheduled / Retrying ジョブに対して手動実行した際、自身が取得した RunnerLock を同一セッション排他検査で `RunnerActive`（競合）と誤認して `SessionBusy` で拒絶される問題を解消すること。ターゲットジョブ自身が自プロセスによって保持しているロックと、別プロセスの競合ロックを安全に区別すること。他のジョブに対する同一セッション排他および過去のクラッシュした Codex 子プロセスの保護は維持すること。
  - **RunnerInfo 読み取り・書き込み失敗を Unknown として扱う**:
    `RunnerInfo` の読み取りにおいて、ファイル不存在（`Missing`）、正常（`Present`）、読み取り/パース失敗（`Unreadable`）を明確に区別すること。読み取り/パース失敗時は `Unknown` として fail-closed に扱い、孤立回復を保留し二重 writer 起動を防止すること。メタデータの書き込みは一時ファイル経由のアトミックな rename で行い、部分的な破損 JSON を露出させないこと。書き込み失敗を無視しないこと。
  - **Fixed 15-second stale decision の廃止**:
    「15秒経過かつ info なし => Dead」という固定タイムアウトによる推測判定を完全に廃止すること。ジョブクレームから detached runner ハンドオフまでの状態を、durable lease / runner process identity（PID + start time）/ システム再起動判定（boot timestamp）等の客観的・永続的エビデンスに基づいて判定すること。時間経過だけを理由に Dead と判定しないこと。
  - **Strict log cap を既存ファイルにも適用**:
    `CappedLogWriter` 初期化時に既存ファイルのサイズを計上し、ファイル全体で 10MB（`MAX_LOG_FILE_BYTES`）を厳密に超えないこと。既存 9MB ファイルへの追記であっても、合計サイズが 10MB 以下にとどまること。
  - **Observability correction**:
    `show --json` において、`RunnerLock` が active であれば `liveness_state` を `"runner_active"` とし、`get_job_liveness()` と矛盾・重複しない一貫した出力を提供すること。
- **Constraint (Human Intent)**:
  - 実行中プロセスの強制終了（SIGKILL 等）は explicit non-scope を維持する。
  - 単一 valid JSON 出力契約および既存フィールドの後方互換性を破壊しない。
  - release / tag / publish は行わない。
- **Agent Decision**:
  - **Self-lock Identification (`OS-SCHED-003`, `CLI-CMD-002`)**:
    `claim_job_for_execution` において、同一セッションの他ジョブ（`other.id != target.id`）については `Running` または `liveness != Dead` であれば `SessionBusy` で拒絶する。ターゲットジョブ自身（`target.id`）については、過去の記録された Codex 子プロセスが `Dead` 以外（`ChildActive` または `Unknown`）であれば拒絶する。ターゲットの `RunnerLock` が保持されている場合、ロック保持者 PID が自プロセス（`std::process::id()`）であれば自プロセスの正当なハンドオフとみなしクレームを許可する。他プロセスのロックであれば `AlreadyRunning` / `SessionBusy` で拒絶する。
  - **Checked RunnerInfo and Atomic Persistence (`SCHED-JOB-006`)**:
    `RunnerInfoRead` enum（`Present(RunnerInfo)`, `Missing`, `Unreadable(String)`）を導入。`Unreadable` の場合は `LivenessState::Unknown` を返し、fail-closed で実行を保護する。`write_runner_info` は一時ファイル（`<job_id>.json.tmp.<pid>`）への書き込み・flush 後に `fs::rename` でアトミックに置き換える。書き込みエラーは無視せず伝播する。
  - **Durable Handoff Lease and Boot Identity (`SCHED-JOB-006`, `SCHED-JOB-007`)**:
    スケジューラ tick がジョブをクレームし detached runner を spawn する際、永続 lease（`HandoffLease`: runner PID, runner start time, boot timestamp）を発行する。孤立判定（reconcile）では：
    1. システムのブート時刻が lease 記録時と異なる場合 => 再起動によるプロセス消滅が確定的であるため `Dead` と判定して安全に回復。
    2. runner PID が記録されている場合 => OS にプロセスの実在と開始時刻を問い合わせ、実在すれば `RunnerActive`（起動処理中）として維持。存在しなければ runner 終了と確定。
    3. 時間経過（15秒等）のみによる Dead 判定は一切行わない。
  - **Strict Existing File Capped Log Writer (`CODEX-RESUME-009`)**:
    `CappedLogWriter::new` において、渡されたファイルの既存サイズ `existing_len` を取得し、`written_bytes = existing_len` から開始する。`existing_len >= max_bytes` であれば追記を即時遮断し、合算ファイルサイズが `max_bytes` を厳密に超えないことを保証する。
  - **CLI Liveness Consistency (`CLI-CMD-003`)**:
    `show --json` で `runner::is_runner_active(&job.id)` が true であれば `liveness_state` を最優先で `"runner_active"` とし、`get_job_liveness()` と完全に整合させる。

## 提案する差分（Proposed Delta）

### 1. `docs/specs/job-lifecycle.md`

#### SCHED-JOB-006: 長時間ジョブ実行ランナー・メタデータ健全性および生存性保証（改訂）
- Runner のメタデータ（`RunnerInfo`）読み取り結果は、`Present`、`Missing`、`Unreadable` の三態を識別しなければならない（MUST）。
- `Unreadable`（ファイル読み取りエラー、JSON 破損等）が検出された場合、生存性判定は `Unknown` を返さなければならない（MUST）。
- `RunnerInfo` の永続化は、アトミックな一時ファイル生成とリネームにより行われなければならず（MUST）、不完全な JSON を公開してはならない（MUST NOT）。メタデータ書き込み失敗を黙殺してはならない（MUST NOT）。
- 手動デバッグ実行（`run-job`）において、自プロセスが保持する RunnerLock は自己ロックとして識別され、クレーム処理を自己妨害してはならない（MUST NOT）。

#### SCHED-JOB-007: クラッシュおよび孤立ジョブの耐久的エビデンスに基づく自動回復（改訂）
- スケジューラ tick における孤立回復判定（reconcile）は、客観的かつ耐久的なエビデンス（システムブート識別子、OS プロセス生存確認、ファイルロック）に基づいて行われなければならない（MUST）。
- 単なる時間経過（例: 15秒経過）のみを根拠としてジョブを Dead と判定し回復してはならない（MUST NOT）。
- システムブート識別子の変更（マシン再起動）が検出された場合は、旧プロセスが全滅したことが確定的であるため、安全に回復を行わなければならない（MUST）。
- Runner プロセスの PID が記録されている場合、OS 上での該当プロセスの実在性を確認し、生存中は回復を行ってはならない（MUST NOT）。
- `Unknown` 状態のジョブを回復してはならない（MUST NOT）。

---

### 2. `docs/specs/os-scheduler.md`

#### OS-SCHED-003: 同一セッション単一ライター保護の全エントリポイント強制および自己ロック識別（改訂）
- すべての実行エントリポイントにおいて、同一 `session_id` を持つ他のジョブのアクティブ実行（`Running` または `liveness_state != Dead`）が存在する場合、クレームを拒絶しなければならない（MUST）。
- ターゲットジョブ自身の手動クレームにおいて、呼び出し元自プロセスが既に RunnerLock を保持している正当なハンドオフである場合、自己競合と誤認して拒絶してはならない（MUST NOT）。
- ターゲットジョブ自身に対して過去の Codex 子プロセスが生存または Unknown である場合は、二重 writer 防止のため拒絶しなければならない（MUST）。

---

### 3. `docs/specs/codex-adapter.md`

#### CODEX-RESUME-009: ストリーミング出力、安全なログ保持、および既存ログを含めた厳格な上限適用（改訂）
- 既存ログファイルへの追記を行う場合であっても、ディスクログファイル全体のサイズは `MAX_LOG_FILE_BYTES`（10MB = 10,485,760 バイト）を厳密に超えてはならない（MUST NOT）。
- 初期化時に既存ファイルのサイズを計上し、残存許容量を超過する出力は切り捨てられなければならない（MUST）。

---

### 4. `docs/specs/cli.md`

#### CLI-CMD-002: ジョブ管理コマンド群（改訂）
- `run-job <job-id>` は、待機中（Scheduled）またはリトライ中（Retrying）のジョブに対しても、手動デバッグ実行として正常に実行可能でなければならない（MUST）。

#### CLI-CMD-003: 機械可読 JSON 出力モード（改訂）
- `show <job-id> --json`:
  - `active_execution` において、RunnerLock が保持されている場合は `liveness_state` に `"runner_active"` を出力し、`codex-scheduler-core` の `get_job_liveness()` と完全に一致させなければならない（MUST）。

## 互換性（Compatibility Impact）

- `jobs.json` の永続化スキーマは完全維持。
- CLI の `show --json` は既存のプロパティスキーマに準拠し、整合性を向上させる。
- 既存のコマンド体系・引数仕様と完全互換。

## 承認適格性（Approval Eligibility）

- **Autonomous approval eligible**: Yes
- **Human gate**: None (Blocking 解消、客観的耐久エビデンス化、排他安全性向上、非破壊的設計)

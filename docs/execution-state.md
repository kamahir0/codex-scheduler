# Development State

Stage: verification-ready
Candidate: 0c69fc6f048439517299552e15eb531a67f8419a
Work base: 67710c63fd2257f3248aee2f7804a267aed5d0d1

## Active work

- Current candidate: 0c69fc6f048439517299552e15eb531a67f8419a
- Implemented Human-approved OS execution container architecture to objectively distinguish Case A vs Case B:
  1. macOS/Unix: runner-owned Process Group container (`PGID == runner_pid`); Codex child inherits PGID; surviving PGID members inspection via `pgrep -g` with PID reuse validation
  2. Windows: per-job named Job Object container (`Local\codex-scheduler-job-<job_id>`); `setup_runner_job_object` (`AssignProcessToJobObject`) before spawn; `find_surviving_job_object_pids` via `QueryInformationJobObject` (`JobObjectBasicProcessIdList`) for surviving processes; no `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`
  3. Reconcile & liveness: use active execution container evidence (`ContainerLiveness`) rather than timeouts to objectively distinguish Case A (safe recovery) from Case B (active child protection)
  4. Verified across 10 failure matrix tests (test_failure_matrix_01 to 10), full workspace tests (126 passed), Windows cross-compilation, and xtask check-all
- Next: Human Acceptance

# Canonical Specifications

このディレクトリは、`codex-scheduler` の承認済み仕様（Approved behavior）を所有する。
すべての要件は一意かつ安定したRequirement ID（例: `SCHED-JOB-001`, `CODEX-RESUME-001` 等）を持つ。

## Specification Index

- [`job-lifecycle.md`](job-lifecycle.md): ジョブのデータモデル、状態遷移、永続化仕様。
- [`codex-adapter.md`](codex-adapter.md): Codex CLIとの連携、コマンド引数組み立て、実行、エラー判定。
- [`retry-policy.md`](retry-policy.md): リトライ判定、試行回数制御、遅延間隔アルゴリズム。
- [`os-scheduler.md`](os-scheduler.md): macOS launchd / Windows Task Scheduler のジョブ登録とworker呼び出し。

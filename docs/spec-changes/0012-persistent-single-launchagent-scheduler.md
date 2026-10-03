# 仕様変更: 単一常設LaunchAgentによるバックグラウンド通知の最小化とScheduler Worker統合

Status: Applied

## Affected Specifications

- `docs/specs/os-scheduler.md`: OS-SCHED-001 (macOS launchd 連携), OS-SCHED-003 (Worker プロセスの独立性)
- `docs/setup-guide.md`: LaunchAgent 登録と通知に関する解説
- `crates/codex-scheduler-core/src/os_scheduler/macos.rs`: 単一常設 LaunchAgent 管理と既存ジョブ plist クリーンアップ
- `crates/codex-scheduler-core/src/os_scheduler/mod.rs`: `SchedulerBackend` トレイトの改定
- `crates/codex-scheduler-cli/src/main.rs`: `tick` サブコマンドの追加
- `crates/codex-scheduler-core/src/lib.rs`: `SchedulerService` のジョブ登録・更新ロジック
- `apps/gui/src-tauri/src/lib.rs`: 診断情報および初期化フローの統合

## 根拠と分類（Source Evidence and Classification）

- **Human Decision / Direct Request**:
  - ジョブ作成のたびにmacOSから「バックグラウンド項目が追加されました」と通知される問題を解消する。
  - ジョブごとのLaunchAgent（`1 Job = 1 LaunchAgent`）を廃止し、アプリ全体で1つの固定LaunchAgent（`~/Library/LaunchAgents/dev.codexscheduler.scheduler.plist`）に統合する。
  - 構造:
    - Codex Scheduler GUI / CLI は `jobs.json` を更新するのみとする。
    - 単一の LaunchAgent（`dev.codexscheduler.scheduler`）が定期的に固定Worker（`codex-scheduler-cli tick`）を起動し、Worker内部で待機中ジョブ（Job A, Job B, Job C...）を管理・実行する。
    - 初回インストール／Worker配置時に1回だけLaunchAgentを登録（初回のみ通知）し、以後のジョブ追加・編集・削除・更新ではLaunchAgentを操作しないため通知を完全に抑制する。
- **Agent Decision**:
  - **実行方式**:
    - LaunchAgent では `StartInterval: 60`（1分間隔）を指定し、`codex-scheduler-cli tick` を定期起動する。
    - 実行対象のジョブがない場合は数ミリ秒で即座に終了するため、常駐メモリ・CPU負荷はほぼゼロとなる。
    - プロセス常駐型デーモンに比べ、クラッシュ復帰やCLIバイナリ更新時のプロセス再起動制御が不要で堅牢。
  - **マイグレーション**:
    - 単一LaunchAgentの登録・確認処理（`ensure_scheduler_installed`）時に、過去のジョブ個別plist（`com.codexscheduler.job.*.plist`）を自動検出し、`launchctl unload` およびファイル削除を行ってクリーンアップする。
  - **冪等性**:
    - plistが既に存在し設定が最新であれば `launchctl load` は再実行しない（無駄なOS通知の再発を防止）。

## 提案する差分（Proposed Delta）

- `docs/specs/os-scheduler.md`:
  - `OS-SCHED-001` を改定:
    - ジョブごとの個別plist生成を廃止。
    - 固定LaunchAgent `~/Library/LaunchAgents/dev.codexscheduler.scheduler.plist`（ラベル `dev.codexscheduler.scheduler`）を1つのみ登録する仕様へ変更。
    - `StartInterval: 60` 秒で `codex-scheduler-cli tick` を起動。
    - ジョブ登録・キャンセル・削除時は `jobs.json` の更新のみ行い、LaunchAgent のロード/アンロードは発生させない。
- `crates/codex-scheduler-core`:
  - `os_scheduler`: `SchedulerBackend` に `ensure_scheduler_installed(cli_path: &Path)` および `uninstall_scheduler()` を定義し、個別の `register_job` / `unregister_job` を不要化（または内部で ensure を呼ぶのみにする）。
  - `os_scheduler/macos.rs`:
    - `dev.codexscheduler.scheduler.plist` の生成と管理。
    - `cleanup_legacy_job_plists()` で古い `com.codexscheduler.job.*.plist` を一括アンロード・削除。
- `crates/codex-scheduler-cli`:
  - `Commands::Tick`:
    - `jobs.json` から `scheduled_at <= Utc::now()` かつ `status == Scheduled` のジョブを抽出し、順次実行する定期Workerコマンドを追加。
- `crates/codex-scheduler-core/src/lib.rs`:
  - `schedule_job`, `cancel_job`, `delete_job`, `execute_job` において、ジョブごとの個別 OS スケジューラ登録・解除処理を廃止し、`ensure_scheduler_installed` の呼び出し（未設定時のみ）に移行。

## 互換性（Compatibility）

- 既存の `jobs.json` スキーマおよびジョブモデルに変更はなく完全な互換性を維持。
- 過去に作成された未実行のジョブは、古いLaunchAgentが削除されても、新Worker（`tick`）によって予定時刻に問題なく検出・実行される。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの直接の指示・設計指定に基づく).
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーの「1 job = 1 LaunchAgent をやめて 1 app = 1 persistent scheduler LaunchAgent にする」という明確なアーキテクチャ指定。
- **Status Transition**: `Proposed` -> `Approved`

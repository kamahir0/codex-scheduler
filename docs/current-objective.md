# Current Objective

## Objective

**Windows Task Scheduler persistent backend + Windows standalone CLI production distribution + cross-platform single scheduler ownership completion**

Windows Task Scheduler production backendを実装し、Windows Desktop / standalone CLIの双方で「アプリ/CLIを閉じても予約ジョブが自動実行される」状態を完成させる。さらにWindows standalone CLIを正式distributionとして追加し、macOS / WindowsでDesktopとCLIが独立インストール可能、かつ各OSでpersistent schedulerを単一ownerとして共有するarchitectureを完成させる。

## Completion slices

- **Slice 1: State Matrix 確定 & 仕様策定・自律承認**:
  - `docs/spec-changes/0016-windows-task-scheduler-and-cli-distribution.md`: 全状態（13行+α）の State Matrix、タスク定義（`CodexScheduler_Service`, canonical recurrence/XML/action/security/settings）、境界契約（`schtasks.exe` vector invocation, `TaskSchedulerRunner` seam）、Desktop precedence / safe migration、CLI behavior、配布形式（4正式アセット）の策定と自律承認。
  - 関連仕様改定: `docs/specs/os-scheduler.md`, `docs/specs/cli.md`, `docs/specs/desktop-delivery.md`, `docs/gui/setup-and-diagnostics.md`, `docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`。
- **Slice 2: コアスケジューラ Windows バックエンド実装**:
  - `crates/codex-scheduler-core/src/os_scheduler/windows.rs`:
    - `TaskSchedulerRunner` トレイトおよび `RealTaskSchedulerRunner`、`MockTaskSchedulerRunner`。
    - `WindowsTaskScheduler` 構造体および `SchedulerBackend` トレイトの実装（ensure, is_installed, is_ready, is_path_matched, get_owner, executable_path, is_target_executable_exists, is_owner_target_valid, uninstall, uninstall_as_cli）。
    - XML 構造解析・生成、State Matrix に基づく safe migration / safe repair / stale-invalid 非破壊保護。
  - `crates/codex-scheduler-core/src/os_scheduler/mod.rs`: `get_platform_scheduler()` の Windows 分岐（`WindowsTaskScheduler` を返却）、`supports_persistent_scheduler()` トレイトメソッドの導入。
  - `crates/codex-scheduler-core/src/lib.rs` (`SchedulerService`): `schedule_job()` の readiness atomic check を platform capability（`supports_persistent_scheduler()`）ベースに統合。
- **Slice 3: CLI 実装 & クロスプラットフォーム化**:
  - `crates/codex-scheduler-cli/src/main.rs`:
    - `status --json`、`install-scheduler`、`uninstall-scheduler` の Windows 対応とエラーメッセージのプラットフォーム中立化。
    - 推奨配置場所（`%USERPROFILE%\.local\bin\codex-scheduler.exe`）の案内と整合性。
- **Slice 4: Desktop GUI & レガシー Worker 整理**:
  - `apps/gui/src/App.tsx`, `CreateJobModal.tsx`, `DiagnosticsModal.tsx`: Windows での「Task Scheduler 未対応」表記を削除し、Task Scheduler による自動定期実行を正しく表示。
  - `apps/gui/src-tauri/tauri.conf.json`: 未使用の bundled worker resources を整理。
  - Windows Desktop uninstall cleanup 調査。
- **Slice 5: テスト・CI ワークフロー・配布パイプライン更新**:
  - `crates/codex-scheduler-core/src/os_scheduler/windows.rs` のユニットテスト（State Matrix 全ケース網羅）。
  - `.github/workflows/ci.yml`: `windows-latest` での Rust テストおよび Task Scheduler 動作検証ステップの追加。
  - `.github/workflows/release.yml`: Windows standalone CLI 正式アセット化、worker staging 削除、4正式アセット配布およびリリース本文の更新。
  - `README.md`, `docs/setup-guide.md` の更新。
- **Slice 6: 検証・Preflight・Adversarial Review**:
  - 全メカニカルチェック合格 (`cargo xtask check-all`)。
  - `cargo xtask release minor --dry-run` (v0.4.1 -> v0.5.0) 計画確認。
  - Release workflow preflight (手動 dispatch による tag なしビルド確認)。
  - Adversarial Review (BLOCKER / PATCH / FUTURE)。
  - Development State を `objective-complete` へ遷移し、Human Release Gate にて停止。

## Authority / output routing

- Canonical CLI specification: [`docs/specs/cli.md`](specs/cli.md)
- OS Scheduler specification: [`docs/specs/os-scheduler.md`](specs/os-scheduler.md)
- Delivery specification: [`docs/specs/desktop-delivery.md`](specs/desktop-delivery.md)
- Architecture decision record: [`docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`](adr/0004-independent-desktop-cli-single-scheduler-owner.md)
- GUI specifications: [`docs/gui/setup-and-diagnostics.md`](gui/setup-and-diagnostics.md)

## Explicit non-scope

- アプリ内自動更新（In-app updater / Tauri updater）。
- CLI 自己更新（CLI self-update）。
- Apple Developer ID / 公証（Notarization）。
- Windows コード署名証明書の購入。
- Windows Service / 常駐バックグラウンドプロセスの新設。
- ジョブごとの個別 Scheduled Task（単一タスク `CodexScheduler_Service` 不変条件を厳守）。
- リモートスケジューラ連携。
- `jobs.json` の破壊的スキーマ変更。
- ポーリング間隔（60秒）の変更。
- 新規 Provider アダプタの追加。

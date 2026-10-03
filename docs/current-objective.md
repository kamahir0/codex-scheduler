# Current Objective

## Objective

**v0.4.0 post-release scheduler health hardening: macOS standalone CLIにおいて、Desktop所有スケジューラがunloaded/unhealthyである場合の安全な再ロード（repair）とスケジュールアトミック性（readiness未達時のジョブ保存防止）を確立し、`status --json` の健全性セマンティクスを自動化安全（automation-safe）にする。**

## Completion slices

- **Slice 1: 仕様策定 & 規範仕様改定**:
  - `docs/spec-changes/0015-macos-scheduler-health-and-cli-status.md`: macOSスケジューラ健全性状態（A: healthy, B: unloaded/recoverable, C: malformed/invalid）、安全修復（repair）手順、スケジュールアトミック性、`install-scheduler` 挙動、`status --json` の後方互換・自動化安全フィールド拡張の仕様提案・承認記録。
  - `docs/specs/cli.md`: `CLI-CMD-003`（status スキーマ定義・`path_matched` セマンティクス明確化と新ヘルスフィールド）、`CLI-CMD-004`（Desktop所有スケジューラの安全修復およびスケジュール失敗時のアトミック性保証）の改定。
  - `docs/specs/os-scheduler.md`: `OS-SCHED-006`（Desktop 所有スケジューラの健全性判定および既存 plist 保持型 repair）の改定。
  - `docs/gui/setup-and-diagnostics.md`, `docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`: 必要に応じた明確化。
- **Slice 2: コアスケジューラ修復・健全性判定・スケジュールアトミック性の実装**:
  - `crates/codex-scheduler-core/src/os_scheduler/macos.rs` および `mod.rs`:
    - Desktop 所有かつ未ロード（unloaded）時の安全な再ロード（plist 内容およびバイナリパスを変更せず `launchctl load -w` による repair）。
    - 登録ターゲット実行ファイルの存在（`target_exists`）および所有権整合性（`owner_target_valid`）判定 API の追加。
    - 単一 LaunchAgent 不変条件と Desktop 所有権の厳格な保護。
  - `crates/codex-scheduler-core/src/lib.rs` (`SchedulerService`):
    - `schedule_job`: スケジューラ健全性確認・safe repair・ready確認の順序保証と、readiness 失敗時に JobStore へジョブを残さないアトミック制御。
- **Slice 3: CLI status / install-scheduler の実装**:
  - `crates/codex-scheduler-cli/src/main.rs`:
    - `status --json`: v0.4.0 公開スキーマとの完全な後方互換性を維持しつつ、`target_exists` / `owner_target_valid` 等の加算（additive）フィールドを追加。`path_matched` の意味（呼び出し元実行ファイルと登録実行ファイルの一致）を明記し、automation が `owner == "desktop"` でも誤判定しない契約を確立。
    - `install-scheduler`: Desktop 所有かつ ready であれば `retained / healthy`、Desktop 所有かつ not ready であれば安全修復を試行し成功時に `repaired`、失敗時に非ゼロ構造化エラーを返却。
- **Slice 4: Desktop GUI Diagnostics 整合性確認**:
  - Desktop GUI の診断モーダル（`DiagnosticsModal.tsx`）および Tauri コマンドが既存のセマンティクスを損なわずに正常動作することを担保。
- **Slice 5: テスト・実機受入計画・検証**:
  - コアおよび CLI の網羅的ユニット・統合テスト追加（Desktop owner loaded / unloaded repair / repair failure / no owner provision / malformed config protection / atomic schedule rollback / status schema compatibility）。
  - `cargo test --workspace`, `npm run frontend:lint`, `npm run frontend:build`, `cargo xtask check-rationale`, `cargo xtask check-all` の全パス。
  - 実機を汚染・破壊しない受入検証計画の策定。
  - `cargo xtask release patch --dry-run` による次 patch（v0.4.1）リリース準備確認。

## Authority / output routing

- Canonical CLI specification: [`docs/specs/cli.md`](specs/cli.md)
- OS Scheduler specification: [`docs/specs/os-scheduler.md`](specs/os-scheduler.md)
- Architecture decision record: [`docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`](adr/0004-independent-desktop-cli-single-scheduler-owner.md)
- GUI specifications: [`docs/gui/setup-and-diagnostics.md`](gui/setup-and-diagnostics.md)

## Explicit non-scope

- Windows Task Scheduler バックエンドの本実装（次期独立 Objective）。
- Windows standalone CLI リリースアセットの追加。
- アプリ内自動更新（In-app updater / Tauri updater）。
- Developer ID 署名および Apple 公証（Notarization）。
- `jobs.json` のスキーマ変更。
- ポーリング間隔（60秒）の変更。
- 2つ目の LaunchAgent 作成（単一 LaunchAgent `dev.codexscheduler.scheduler` 不変条件を厳守）。

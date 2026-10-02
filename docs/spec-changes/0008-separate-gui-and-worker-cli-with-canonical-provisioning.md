# 仕様変更: GUIと常設Worker CLIの権限分離および固定パスプロビジョニング

Status: Applied

## Affected Specifications

- `docs/specs/desktop-delivery.md`: DELIVERY-BUNDLE-002, DELIVERY-BUNDLE-004
- `docs/setup-guide.md`: セクション2 (macOSの権限とGUI・Worker分離設計)
- `apps/gui/src-tauri/tauri.conf.json`: macOS signingIdentity & bundle resources
- `apps/gui/src-tauri/src/lib.rs`: SchedulerService worker path resolution & diagnostics
- `crates/codex-scheduler-core/src/worker.rs`: Canonical worker provisioning module

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - Apple Developer Program なし（完全無料・ad-hoc署名）の範囲でGitHub Releases配布を継続する。
  - GUIアプリ（`.app`）をバージョンアップで置換するたびにmacOSのTCC（プライバシー・フォルダアクセス）権限が再要求される課題を解消する。
  - `launchd` 等のOSスケジューラ登録コマンドがGUI自身（`current_exe`）を誤って指してしまう実装バグを是正する。
- **Agent Decision**:
  - **権限分離アーキテクチャ**:
    - 頻繁に更新・置換されるGUI本体（`/Applications/Codex Scheduler.app`）にはTCC依存の直接処理（深夜バッチ・Codex実行等）を持たせず、ジョブ作成・設定・状態表示に特化する。
    - 実際のスケジュール実行を行う常設Worker CLI（`codex-scheduler-cli`）をユーザー固定領域（`~/.local/share/codex-scheduler/bin/` / `%LOCALAPPDATA%\codex-scheduler\bin\`）へプロビジョニング（コピー・権限付与）する。
    - `launchd` plist / Windows Task Scheduler の `ProgramArguments` にはこの固定Workerパスの絶対パスを登録する。
  - **自動プロビジョニング**:
    - GUI初回起動時または更新時に、アプリ内リソースまたは同一階層に同梱された `codex-scheduler-cli` を検出し、固定パスへ自動配置（Unix: `0o755` 権限付与）。
  - **署名設定**:
    - `tauri.conf.json` の `bundle.macOS.signingIdentity` に `"-"`（ad-hoc署名）を明示設定し、Apple Siliconでの実行可能性を担保。

## 提案する差分（Proposed Delta）

- `docs/specs/desktop-delivery.md`:
  - DELIVERY-BUNDLE-002 を「固定パス配置（Worker Provisioning）と分離アーキテクチャ」に改定。
  - DELIVERY-BUNDLE-004 として「Apple Developerなし配布方針とad-hoc署名」を追加。
- `crates/codex-scheduler-core`:
  - `worker` モジュールを追加し、固定パス解決（`canonical_worker_path`）、同梱バイナリ検出（`find_bundled_worker_binary`）、自動配置（`ensure_worker_installed`）を実装。
- `apps/gui/src-tauri`:
  - `lib.rs` で `current_exe` ではなく `ensure_worker_installed()` のパスを `SchedulerService` に注入。
  - `SystemInfo` に `cli_worker_installed` を追加。
- `apps/gui/src`:
  - `DiagnosticsModal` に Worker インストール状態と分離アーキテクチャの説明を表示。
- `.github/workflows/release.yml`:
  - ビルドした `codex-scheduler-cli` を Tauri バンドル用 resources にステージング。
- `docs/setup-guide.md`:
  - 完全無料構成でのGUI・Worker分離と権限維持メカニズムを解説。

## 互換性（Compatibility）

- 既存のDB（`jobs.json`）スキーマに変更はなく後方互換性を完全維持。
- 既存の `launchd` ジョブは次回更新時または新規登録時に固定Workerパスへ自然に移行。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの明示方針「Apple Developerなし完全無料」および不具合是正に基づく)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーの「apple developerは無しで完全無料の範囲で作る」「実装等を修正」という指示。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

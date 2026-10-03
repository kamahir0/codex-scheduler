# 仕様変更: macOS シングルバイナリ・ヘッドレススケジューラアーキテクチャとGatekeeper拒否根本対策

Status: Approved

## Affected Specifications

- `docs/specs/desktop-delivery.md`: DELIVERY-BUNDLE-002（固定Worker CLI配置の廃止とメインアプリ実行ファイルへの統合）、DELIVERY-BUNDLE-004（権限責任の局所化の改定）
- `docs/specs/os-scheduler.md`: OS-SCHED-001（macOS launchd 単一常設連携における実行バイナリと引数の改定）、OS-SCHED-003（Worker プロセスとヘッドレスモードの定義）
- `docs/gui/setup-and-diagnostics.md`: DIAG-UI-003（システム情報・権限診断モーダルにおけるスケジューラ情報）
- `docs/setup-guide.md`: macOS インストール・権限セットアップ手順（外部Worker CLI不要化）
- `crates/codex-scheduler-core/src/os_scheduler/macos.rs`: LaunchAgent plist 生成とエラーハンドリング、パス更新判定
- `crates/codex-scheduler-core/src/store.rs`: 重複実行抑止（atomic claim）
- `crates/codex-scheduler-core/src/lib.rs`: 共有 `execute_tick` ロジックとスケジューラ登録エラーの伝播
- `apps/gui/src-tauri/src/main.rs`: コマンドライン引数ルーティング（通常GUI vs `--scheduler-tick` ヘッドレス）
- `apps/gui/src-tauri/src/lib.rs`: ヘッドレス tick 実行ルーチンと Tauri 初期化のスキップ

## 根拠と分類（Source Evidence and Classification）

- **Human Decision / Direct Request**:
  - Apple Developer Program の有償署名なし（ad-hoc署名）で配布する環境において、GUI アプリ（`Codex Scheduler.app`）から外部の固定パス（`~/.local/share/codex-scheduler/bin/codex-scheduler-cli`）へコピーされた別 Mach-O バイナリが Gatekeeper によって実行拒否される問題（実機で「“codex-scheduler-cli”は開いていません」と拒否される）を根本解消する。
  - macOS デスクトップ版において、OS スケジューラの実行主体として別バイナリ `codex-scheduler-cli` を使用する方式を廃止する。
  - 「1 app, 1 executable, 2 execution modes」を採用する：
    - `Codex Scheduler.app/Contents/MacOS/<executable>`
    - 通常起動: Tauri GUI
    - バックグラウンド実行（LaunchAgent）: `--scheduler-tick` 引数を付与したヘッドレスモード
  - LaunchAgent plist の `ProgramArguments` は、現在インストールされているアプリ本体の絶対パス（`std::env::current_exe()` 等で取得し、ハードコードしない）と `--scheduler-tick` を指す。
  - 単一常設 LaunchAgent 方式（`1 Application = 1 LaunchAgent`, `N Jobs = jobs store`）を維持する。
  - アプリから `xattr -d com.apple.quarantine` などの Gatekeeper 回避処理を自動実行しない。
  - スケジューラ登録・更新の失敗を握りつぶさず（`let _ = ...` の排除）、structured error として GUI へ伝播させる。
  - アプリの移動やバージョンアップ時に LaunchAgent の plist 内容（ProgramArguments）が変化した場合のみ安全に再登録する（無駄な「バックグラウンド項目が追加されました」通知の再発を防止）。
  - ヘッドレスモード実行中（60秒超など）に重複して同じジョブが二重実行されないよう、排他制御（atomic claim / store lock）を保証する。
- **Agent Decision**:
  - **引数設計**:
    - メイン実行ファイルの CLI 引数として `--scheduler-tick` を受け付け、検出時は Tauri GUI ランタイム・Window・Dock アイコンを生成せず、直ちに非同期コアランタイム上で待機中ジョブの `execute_tick` を実行して終了する。
  - **排他制御（Atomic Claim）**:
    - `JobStore` において、`scheduled_at <= Utc::now()` かつ `status == JobStatus::Scheduled` のジョブを抽出する際、同一トランザクション/ファイルロック内で直ちにステータスを `Running` に遷移させて保存する `claim_due_jobs()` を提供し、並行・連続する tick プロセス間での重複実行を排除する。
  - **レガシーマイグレーション**:
    - `ensure_scheduler_installed` において、既存の `dev.codexscheduler.scheduler.plist` が古い `codex-scheduler-cli` パスを指している場合や古いパスを指している場合は自動的に新メイン実行ファイルのパスへ更新する。
    - 旧バージョンの `com.codexscheduler.job.*.plist` も引き続き自動クリーンアップする。
    - ユーザーの `~/.local/share/codex-scheduler/bin/codex-scheduler-cli` は、CLI単体利用者の破壊を防ぐため無理に削除せず、デスクトップスケジューラ側の実行参照から完全に外す。

## 提案する差分（Proposed Delta）

- `docs/specs/desktop-delivery.md`:
  - `DELIVERY-BUNDLE-002` を改定: 固定パスへの外部 Worker CLI コピー方式を廃止し、macOS デスクトップ版のスケジューラ実行主体はメインアプリ実行ファイル（`Codex Scheduler.app/Contents/MacOS/...`）自身に統合する。
  - `DELIVERY-BUNDLE-004` を改定: 「固定パスだから権限が永続する」という前提記述を改め、単一アプリ実行ファイルを Gatekeeper で1回許可することで、GUI も LaunchAgent ヘッドレス実行も同一コード署名・エンティティとして動作する設計を規定。
- `docs/specs/os-scheduler.md`:
  - `OS-SCHED-001` を改定: `ProgramArguments` を `[<main_app_executable_path>, "--scheduler-tick"]` とし、動的パス解決・差分時のみの更新を規定。
  - `OS-SCHED-003` を改定: ヘッドレスモードの要件（GUI window 非生成、Dock 非表示、Tauri 非初期化、shared core `execute_tick` の実行）を規定。
- `crates/codex-scheduler-core`:
  - `os_scheduler/macos.rs`:
    - `generate_scheduler_plist_content(app_executable_path: &Path)`: 引数を `app_executable_path` と `--scheduler-tick` に更新。
    - `ensure_scheduler_installed`: `launchctl` の失敗時に `SchedulerError::CommandFailed` を返しエラーを握りつぶさない。既存 plist が新 executable パスと不一致の場合は再登録。
  - `store.rs`:
    - `claim_due_jobs(now: DateTime<Utc>) -> Result<Vec<Job>, StoreError>`: 排他的に due job を取得し `Running` に更新して永続化。
  - `lib.rs`:
    - `execute_tick() -> Result<Vec<Job>, CoreError>`: core shared logic として tick 処理を提供。
    - `schedule_job()` で `ensure_scheduler()` のエラーを `?` で伝播。
- `apps/gui/src-tauri`:
  - `main.rs`: 引数に `--scheduler-tick` が含まれる場合は `codex_scheduler_gui_lib::run_headless_tick()` を呼び出して即終了。含まれない場合は通常の `codex_scheduler_gui_lib::run()`。
  - `lib.rs`: `run_headless_tick()` の提供、`create_job` でのスケジューラ登録エラー伝播。
- `docs/setup-guide.md`:
  - Worker CLI のコピー・権限付与に関する古い説明を削除し、アプリ本体を通常通り起動・許可するだけでバックグラウンド実行も動作する説明に更新。

## 互換性（Compatibility）

- 既存の `jobs.json` スキーマおよびジョブステータス遷移（Scheduled -> Running -> Completed/Failed/Retrying）は完全互換。
- 既存の LaunchAgent plist が残っている環境では、次回 GUI 起動時またはジョブ登録時に新しい executable パスへ安全に自動更新される。
- Windows 環境の動作には不要な変更を加えず、既存の Task Scheduler 機構との互換性を保つ。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes（ユーザーの明示的指示・アーキテクチャ指定に基づく）.
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Agent-autonomous (Human-directed)
- **Basis**: ユーザーの明確なプロンプト指示および「1 app, 1 executable, 2 execution modes」の採用指定。
- **Status Transition**: `Proposed` -> `Approved`

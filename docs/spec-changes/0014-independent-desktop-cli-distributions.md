# 仕様変更: Desktop GUI / standalone CLI 独立ディストリビューションと単一スケジューラ所有権アーキテクチャ

Status: Approved

## Affected Specifications

- `docs/specs/cli.md`: 新規策定（CLI-CMD-001 〜 CLI-CMD-005）
- `docs/specs/os-scheduler.md`: OS-SCHED-006（macOS スケジューラ所有権モデルと優先度）の追加、OS-SCHED-001の改定
- `docs/specs/desktop-delivery.md`: DELIVERY-BUNDLE-001（配布パッケージ形式）、DELIVERY-BUNDLE-005（アセット命名規則）の改定
- `docs/gui/setup-and-diagnostics.md`: DIAG-UI-003（システム情報・権限診断におけるスケジューラ所有者の表示）
- `docs/product/vision.md`: 配布モデルおよび製品コンポーネント構成の改定
- `docs/setup-guide.md`: Desktop only / CLI only / 両方共存のセットアップ手順の改定
- `README.md`: 利用モデル（Desktop / CLI / 共存）とインストール方法の改定

## 根拠と分類（Source Evidence and Classification）

- **Human Decision / Direct Request**:
  - Codex Scheduler を独立した正式ディストリビューションとして提供する：
    - **Desktop distribution**: GUIを提供、Desktop単体で完全に予約実行可能、standalone CLIのインストールに依存しない（macOS DMG, Windows EXE）。
    - **CLI distribution**: GUI不要、macOSにおいてCLI単体で完全に予約実行可能、Desktopのインストールに依存しない（macOS Apple Silicon standalone binary。Windows CLI Task Scheduler統合は次期候補）。
  - 両方がインストールされている場合：
    - `JobStore` / コアセマンティクスは共有（`~/.codex-scheduler/jobs.json`）。
    - persistent scheduler は1つだけ（macOS `dev.codexscheduler.scheduler`）。
    - macOS では Desktop-owned scheduler を優先する。
    - CLI は control plane として利用可能。
  - **禁止事項**: 「Desktopが別CLI Workerを必須とする」旧アーキテクチャへ戻してはならない。
  - **macOS 所有権ポリシー（Ownership Policy）**:
    1. 有効な Desktop-owned registration が存在する場合、CLI は絶対に奪わない（MUST NOT overwrite）。
    2. 有効な CLI-owned registration が存在する場合、CLI は冪等な no-op とする。
    3. CLI-owned 環境で Desktop GUI が起動した場合、Desktop が安全に ownership を引き継ぐ（migrate）。
    4. Desktop-owned 環境で CLI を後からインストール・実行した場合、Desktop ownership を維持し、CLI から schedule 可能とする。
    5. 実行ファイルが消失した既知の管理対象 stale registration は、利用可能な frontend が安全に repair / takeover 可能とする。
    6. 未知・不正な設定（unknown / malformed）は、デフォルトで破壊的上書きをせず、構造化された actionable error とする。
  - **Gatekeeper 回避の維持**:
    - macOS Desktop 版は「1 App / 1 Desktop executable / 2 Desktop modes（GUI, `--scheduler-tick`）」を維持。Desktop scheduler が standalone CLI バイナリを呼ぶ構成は禁止。
    - CLI-only installation の場合のみ、LaunchAgent が standalone `codex-scheduler --scheduler-tick` を実行することを許可する。
  - **standalone CLI の正式コマンド名**: `codex-scheduler`。
  - **正式 CLI コマンドサーフェス**:
    - `codex-scheduler schedule`
    - `codex-scheduler list`
    - `codex-scheduler show <job-id>`
    - `codex-scheduler cancel <job-id>`
    - `codex-scheduler delete <job-id>`
    - `codex-scheduler status`
    - `tick`（または `--scheduler-tick`）は内部スケジューラ呼出として維持。
    - `install-scheduler` は CLI-only セットアップ / repair 用途として正式化。
    - `uninstall-scheduler` は ownership-aware とし、Desktop-owned scheduler を standalone CLI から無断削除してはならない（MUST NOT）。
  - **CLI schedule UX**:
    - 既存タイムフォーマット互換維持: RFC3339, `+<minutes>`, `YYYY-MM-DD HH:MM`（local）。
    - `scheduled_at` は OS-SCHED-005（Earliest Execution Time, 60秒ポーリング, no exact-time guarantee）を共有。
  - **CLI-only スケジューラセットアップ**:
    - CLI-only 利用者に Desktop GUI 起動を要求してはならない。CLI 自身で scheduler を安全に ensure できること。
  - **機械可読 CLI (`--json`)**:
    - `--json` フラグをサポートし、成功時は stdout に valid JSON のみ出力（exit 0）。失敗時は stdout に human prose を混ぜず non-zero exit、エラー情報を出力。
  - **Status コマンドスキーマ**:
    - product version, store path, scheduler installed, scheduler ready, scheduler owner (`desktop`, `cli`, `none`, `legacy`, `invalid`), scheduler executable path, scheduler path validity, platform を含む。
  - **Desktop 診断**:
    - Diagnostics UI も ownership model を認識し、所有者（Desktop / CLI）を表示する。
  - **Release Assets**:
    - Desktop: `Codex-Scheduler-<version>-macos-arm64.dmg`, `Codex-Scheduler-<version>-windows-x64.exe`
    - CLI: `Codex-Scheduler-CLI-<version>-macos-arm64`（Windows standalone CLI は Task Scheduler 常設バックエンド未統合のため正式配布対象外とし、Windows では Desktop distribution による予約実行をサポート）
    - macOS CLI アセットは standalone executable として提供し、ad-hoc 署名を適用。
    - インストール案内は user-local 方式（`~/.local/bin/codex-scheduler`）を第一候補とし、sudo 必須にしない。shell rc の自動編集は禁止。
  - **Non-scope**:
    - In-app updater、HTTP/常駐ネットワークサーバー、`xattr -d com.apple.quarantine` 自動実行、Windows Task Scheduler CLI 単体常設統合は明示的スコープ外。
- **Agent Decision**:
  - `SchedulerOwner` 列挙型を `codex-scheduler-core::os_scheduler` に定義（`Desktop`, `Cli`, `None`, `Legacy`, `Invalid`）。
  - macOS における所有者判定は、`dev.codexscheduler.scheduler.plist` の `ProgramArguments` を解析し、登録されたバイナリが macOS App bundle（`.app` 内の実行ファイルまたは `codex-scheduler-gui`）か standalone CLI（`codex-scheduler` または `codex-scheduler-cli`）か、あるいは実在しない/旧形式かを厳格に識別する。
  - CLI `uninstall-scheduler` は、現所有者が `Desktop` の場合エラーを返して保護し、`Cli` の場合のみアンロード・削除を行う。
  - CLI `schedule` コマンド実行時、macOS でスケジューラが未登録または消失している場合は、実行中の CLI バイナリで自動 ensure を試行する。既に有効な Desktop 所有スケジューラが存在する場合は変更を加えずジョブのみ登録する。

## 提案する差分（Proposed Delta）

- **新規仕様 `docs/specs/cli.md`**:
  - `CLI-CMD-001: 正式コマンド名および配布形態`: コマンド名 `codex-scheduler`、macOS において単体で予約実行完結、Desktop 非依存。CLI コントラクト自体は cross-platform に提供。
  - `CLI-CMD-002: 正式コマンドサーフェス`: `schedule`, `list`, `show`, `cancel`, `delete`, `status`, `tick`, `install-scheduler`, `uninstall-scheduler` のパラメータと挙動。
  - `CLI-CMD-003: 機械可読 JSON モード`: `--json` オプション時の純粋 JSON 出力・非ゼロ終了コード・エラー出力規約。
  - `CLI-CMD-004: スケジューラ所有権認識と自己プロビジョニング`: macOS CLI-only での LaunchAgent 自動登録、Desktop 所有スケジューラの尊重、無断削除防止。Windows Task Scheduler 統合は次期 Objective 候補（Non-goal）。
  - `CLI-CMD-005: 共有 JobStore と並行性制御`: `~/.codex-scheduler/jobs.json` の共有、ファイルロックとアトミッククレームによる GUI/CLI 並行安全性の保証。
- **仕様改定 `docs/specs/os-scheduler.md`**:
  - `OS-SCHED-006: macOS スケジューラ所有権モデルと優先度（Single Scheduler Ownership & Precedence）`:
    - 単一 LaunchAgent 不変条件: ラベル `dev.codexscheduler.scheduler` のみ。
    - 所有権優先度ルール（Desktop 優先、CLI は奪わない、Desktop 起動時の安全な引継ぎ、stale 時の修復、未知設定の非破壊エラー）。
    - 実行主体引数: Desktop は `[<app_executable>, "--scheduler-tick"]`、CLI は `[<cli_executable>, "--scheduler-tick"]`。
- **仕様改定 `docs/specs/desktop-delivery.md`**:
  - `DELIVERY-BUNDLE-001`: 配布パッケージ形式を 3 種類（Desktop 2種: macOS DMG, Windows EXE + standalone CLI 1種: macOS arm64 standalone binary）に厳格化。Windows standalone CLI は Task Scheduler 常設バックエンド未統合のため正式配布対象外とし、Windows 環境での予約実行は Desktop distribution を正式サポートとする。
  - `DELIVERY-CI-002`: アセット命名規則に `Codex-Scheduler-CLI-<version>-macos-arm64` を追加（計 3 アセット）。
- **仕様改定 `docs/gui/setup-and-diagnostics.md`**:
  - `DIAG-UI-003`: 診断モーダル項目に「スケジューラ所有者（Desktop / CLI）」および所有権状態を追加。

## 互換性（Compatibility）

- **JobStore スキーマ**: `~/.codex-scheduler/jobs.json` のスキーマは一切変更せず、既存ジョブ・履歴データを完全に維持。
- **下位互換性**: 既存の v0.3.x Desktop-owned LaunchAgent はそのまま Desktop owner として認識・維持される。
- **CLI コマンド互換**: 従来の引数形式（RFC3339, `+<minutes>`, local format）を完全維持。

## 受け入れと実装への影響（Acceptance and Implementation Impact）

- `codex-scheduler-core`:
  - `os_scheduler`: `SchedulerOwner`、`get_scheduler_owner()`、所有権配慮型 `ensure_scheduler_installed` の実装。
- `codex-scheduler-cli`:
  - `Cargo.toml`: `[[bin]] name = "codex-scheduler"`。
  - `main.rs`: 各サブコマンドの実装、`--json` 出力、`status` コマンド、所有権配慮型 `uninstall-scheduler`。
- `apps/gui`:
  - `SystemInfo` に `scheduler_owner` 追加、`DiagnosticsModal` に表示。
  - 起動時の CLI owner からの Desktop への引継ぎ。
- `.github/workflows/release.yml`:
  - standalone CLI の build, ad-hoc sign, staging。

## 未解決事項（Open Questions）

- None identified. 全て Human Request および Agent Decision により解決済み。

## レビュー（Review）

- **Blocking Issues**: None identified.
- **Non-blocking Issues**: None identified.
- **Questions**: None identified.
- **Approved as Proposed**: Yes.
- **Autonomous approval eligibility**:
  - Eligible: Yes
  - Human gate: None（Human による新 Objective 選択および指示に基づく）
  - Rationale: Human 明示指示に基づくアーキテクチャ・仕様であり、Human gate 条件に該当しない。

## 承認記録（Approval Record）

- **Approval mode**: Agent-autonomous (Human-directed)
- **Basis**: 新 Objective に関する Human prompt の明示的設計指示。
- **Status Transition**: `Draft` -> `Proposed` -> `Approved`

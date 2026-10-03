# 仕様変更提案 0016: Windows Task Scheduler 本番バックエンド統合と Windows Standalone CLI 正式配布

- Status: Implemented
- Date: 2026-10-03
- Author: AI Assistant (Agent-autonomous approval under Human-authorized Objective)
- Affected Specifications:
  - `docs/specs/os-scheduler.md`
  - `docs/specs/cli.md`
  - `docs/specs/desktop-delivery.md`
  - `docs/gui/setup-and-diagnostics.md`
  - `docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`

---

## 1. 概要と背景

本変更は、Human-selected Objective **「Windows Task Scheduler persistent backend + Windows standalone CLI production distribution + cross-platform single scheduler ownership completion」** を達成するための仕様改定である。

現行（v0.4.1）において、macOS では単一 LaunchAgent（`dev.codexscheduler.scheduler`）による常設バックグラウンド実行が確立されているが、Windows では `FallbackScheduler`（内部 no-op）のままであり、常設スケジューラ連携は未実装であった。
本仕様により、Windows において単一常設タスク（`CodexScheduler_Service`）による Task Scheduler 本番バックエンドを統合し、Desktop GUI と standalone CLI の双方が独立してインストール可能でありながら、共通の `JobStore`（`%USERPROFILE%\.codex-scheduler\jobs.json`）および単一の常設スケジューラを共有するアーキテクチャを完成させる。

---

## 2. 状態遷移マトリクス（Windows Scheduler State Matrix）

Windows 環境において、タスクスケジューラ登録（`ensure_scheduler_installed`）および修復・移行処理は、以下の状態マトリクスに厳格に従わなければならない（MUST）。

| ID | Existing task owner | Task config | Target binary | Enabled/Healthy | Caller | Expected behavior & transition |
|:---|:---|:---|:---|:---|:---|:---|
| **ST-01** | `None` | なし | - | no | CLI | 新規 CLI owner タスクとして作成・登録（`codex-scheduler.exe --scheduler-tick`） |
| **ST-02** | `None` | なし | - | no | Desktop | 新規 Desktop owner タスクとして作成・登録（`codex-scheduler-gui.exe --scheduler-tick`） |
| **ST-03** | `Desktop` | canonical | exists | yes (ready) | CLI | **Retain**: タスク内容・所有権を変更せず `Ok(())`。新規ジョブは共有 `jobs.json` へ保存 |
| **ST-04** | `Desktop` | canonical | exists | no (disabled/unready) | CLI | **Safe Repair**: Desktop 所有権およびターゲットパスを維持したまま、タスクを有効化・再登録して repair。失敗時はジョブ保存禁止 |
| **ST-05** | `Desktop` | canonical | exists | yes/no | Desktop | **Idempotent / Self-repair**: 同一構成なら retain、無効・不整合なら Desktop owner のまま更新・有効化 |
| **ST-06** | `Cli` | canonical | exists | yes (ready) | CLI | **Retain**: 同一 CLI owner のまま維持 |
| **ST-07** | `Cli` | canonical | exists | no (disabled/unready) | CLI | **Self-repair**: CLI owner のままタスクを有効化・再登録して repair |
| **ST-08** | `Cli` | canonical | exists | any (yes/no) | Desktop | **Safe Takeover**: Desktop GUI 起動時に、既存 CLI タスクを Desktop owner タスク（`codex-scheduler-gui.exe`）へ安全に移行 |
| **ST-09** | `stale Desktop` | managed | missing | no | CLI | **Error / No Takeover**: ターゲット不存在エラー（`ExecutableNotFound`）。CLI による上書き・所有権奪取を禁止。ジョブ保存禁止 |
| **ST-10** | `stale Desktop` | managed | missing | no | Desktop | **Desktop Recovery**: 現在実行中の Desktop アプリ絶対パスでタスクを再登録・修復 |
| **ST-11** | `stale CLI` | managed | missing | no | CLI | **CLI Recovery**: 現在実行中の正規 CLI パスでタスクを再登録・修復 |
| **ST-12** | `stale CLI` | managed | missing | no | Desktop | **Safe Takeover**: Desktop GUI により、Desktop owner タスクへ移行・上書き |
| **ST-13** | `Invalid / Unknown`| malformed | ? | ? | any | **Malformed Protection**: 未知の設定・構文不正タスクは上書きせず構造化エラー（`MalformedConfiguration`）を報告 |

---

## 3. Windows Task Scheduler 規範要件

### 3.1. タスク識別・不変条件
- **固定タスク名**: `CodexScheduler_Service`
- **単一タスク不変条件**: 1 Application = 1 persistent scheduled task, N Jobs = `jobs.json` 内部管理。ジョブごとの個別 Scheduled Task 作成は禁止（MUST NOT）。

### 3.2. タスク構成（Canonical Task Definition）
- **Trigger**:
  - `Repetition.Interval`: `PT1M`（1分間隔）
  - `Repetition.Duration`: 無制限（常設実行）
  - `Enabled`: `true`
- **Action**:
  - `Exec.Command`: 実行ファイル絶対パス（クォートまたは引数分離）
  - `Exec.Arguments`: `--scheduler-tick`
- **Security Context**:
  - Current User context（管理者昇格 UAC、SYSTEM 実行、パスワード保存を要求しない Least Privilege 原則）。
  - `LogonType`: `InteractiveToken`
- **Settings**:
  - `MultipleInstancesPolicy`: `IgnoreNew`
  - `DisallowStartIfOnBatteries`: `false`
  - `StopIfGoingOnBatteries`: `false`
  - `RunOnlyIfIdle`: `false`
  - `RunOnlyIfNetworkAvailable`: `false`
  - `StartWhenAvailable`: `true`
  - `WakeToRun`: `false`（macOS 同様、PC sleep 中の exact execution は保証せず、復帰後次 tick で処理）

### 3.3. OS 境界とコマンド実行契約
- Windows 標準 `schtasks.exe` を使用し、`std::process::Command` の引数ベクタ形式で呼び出す（shell 文字列結合によるコマンドインジェクションを根本排除）。
- タスク照会は `schtasks /Query /TN CodexScheduler_Service /XML` を使用し、XML を構造的に解析する。
- 抽象化レイヤ `TaskSchedulerRunner` トレイトを導入し、テスト時は `MockTaskSchedulerRunner` により実機 Scheduled Task を汚染せず決定論的に検証可能とする。

---

## 4. 所有権モデルと実行ファイル判定

- **Desktop Owner**:
  - タスクのアクション実行ファイル名が `codex-scheduler-gui.exe`（大文字小文字不問）
  - 実行ファイルが実在する
  - 引数に `--scheduler-tick` を含む
- **CLI Owner**:
  - タスクのアクション実行ファイル名が `codex-scheduler.exe`（大文字小文字不問、または legacy互換名 `codex-scheduler-cli.exe`）
  - 実行ファイルが実在する
  - 引数に `--scheduler-tick` を含む
- **None**: タスク `CodexScheduler_Service` が存在しない
- **Invalid**: タスクは存在するがアクション不存在、引数不正、実行ファイル消失（stale）、または未認識バイナリ

---

## 5. デスクトップ優先度と安全な移行（Desktop Precedence & Safe Takeover）

1. **Desktop 優先（Desktop Precedence）**:
   - Desktop 所有の有効なタスクが存在する場合、CLI からの呼び出しはタスク内容を変更してはならない（MUST NOT overwrite）。
   - 未ロード・無効化状態（State ST-04）の場合、CLI は Desktop 所有権およびターゲットパスを変更せず、同一タスクを有効化・修復（safe repair）する。
2. **CLI から Desktop への安全な移行（Safe Takeover / State ST-08）**:
   - CLI 所有タスクが存在する環境で Desktop GUI が起動された場合、Desktop はタスクの実行対象を `codex-scheduler-gui.exe` へ安全に移行する。
   - 移行処理は transaction-safe に行い、既存タスク定義を取得した上で更新を行う。

---

## 6. スケジュールアトミック性とプラットフォーム共通化

- `SchedulerBackend` トレイトに `supports_persistent_scheduler(&self) -> bool` を追加（macOS: `true`, Windows: `true`, Fallback: `false`）。
- `SchedulerService::schedule_job` は `supports_persistent_scheduler()` が `true` のプラットフォームにおいて、スケジューラ準備完了（ready）を確認後にのみ `JobStore` への書き込みを行う。修復失敗時や設定破損時は新規ジョブを保存せずエラーを返却する。

---

## 7. 配布パッケージとリリースアセット（Delivery & Packaging）

GitHub Releases における正式配布アセットを以下の **exactly 4 assets** とする：

1. `Codex-Scheduler-<version>-macos-arm64.dmg` (macOS Desktop GUI)
2. `Codex-Scheduler-<version>-windows-x64.exe` (Windows Desktop GUI NSIS)
3. `Codex-Scheduler-CLI-<version>-macos-arm64` (macOS standalone CLI)
4. `Codex-Scheduler-CLI-<version>-windows-x64.exe` (Windows standalone CLI)

### レガシー Worker リソースの整理
- Windows Desktop アプリはスタンドアロン CLI を runtime dependency としないため、release workflow における `Stage worker CLI for bundle (Windows only)` および `tauri.conf.json` の不要な `resources` 定義を削除・整理する。

---

## 8. CLI コマンド契約と推奨インストールパス

- Windows standalone CLI の推奨配置場所:
  `%USERPROFILE%\.local\bin\codex-scheduler.exe`
- 配布ファイル名 `Codex-Scheduler-CLI-<version>-windows-x64.exe` は、配置時に `codex-scheduler.exe` にリネームして利用するようセットアップガイドに明記。
- CLI コマンド（`schedule`, `list`, `show`, `cancel`, `delete`, `status`, `tick`, `install-scheduler`, `uninstall-scheduler`）は macOS と同様に完全機能する。
- `uninstall-scheduler` は Desktop 所有タスクに対して保護エラー（`desktop_owner_protected`）を返す。

---

## 9. 承認分類（Approval Eligibility）

- **Human gate**: None（Human-selected Objective のスコープ内、既存アーキテクチャのWindows拡張）
- **Approval mode**: Agent-autonomous
- **Status**: Approved

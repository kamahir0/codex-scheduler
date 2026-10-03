# 仕様変更: macOS スケジューラ健全性検証・安全修復および CLI Status 自動化安全化

Status: Approved

## Approval Record

- **Approval mode**: Agent-autonomous
- **Approval basis**: Human-selected Objective "v0.4.0 post-release scheduler health hardening"
- **Review result**:
  - Blocking Issues: None identified
  - Non-blocking Issues: None identified
  - Questions: None identified
  - Approved as Proposed: Yes
  - Autonomous approval eligibility: Eligible (Human gate: None)

## Affected Specifications

- `docs/specs/cli.md`:
  - `CLI-CMD-003`: `status --json` スキーマの明確化（`path_matched` セマンティクスおよび `target_exists`, `owner_target_valid` の加算）、自動化セーフな健全性判定規則の追加。
  - `CLI-CMD-004`: Desktop 所有スケジューラの健全性検証、未ロード（unloaded）時の安全な再ロード（safe repair）、スケジュール失敗時のアトミック性（JobStore 保存防止）の追加。
- `docs/specs/os-scheduler.md`:
  - `OS-SCHED-006`: macOS スケジューラ所有権モデルと優先度における 3 状態モデル（Healthy, Unloaded/Recoverable, Malformed/Invalid）の定義、既存 plist 保持型修復（Repair）、readiness 不足時の新規スケジュール防止。
- `docs/gui/setup-and-diagnostics.md`:
  - 診断モーダルにおける `path_matched` およびスケジューラ健全性表示の整合性確認・明確化。
- `docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`:
  - Desktop 優先原則における「有効（valid + operational）な Desktop スケジューラ」の定義追補。

## 根拠と分類（Source Evidence and Classification）

- **Human Decision (Request)**:
  - Desktop owner 優先（Desktop Precedence）、単一 LaunchAgent（`dev.codexscheduler.scheduler`）、CLI は Desktop owner を奪わない（MUST NOT overwrite with CLI binary）という決定を維持する。
  - 尊重すべきなのは「valid + operational (loaded/ready)」な Desktop scheduler であり、未ロード（unloaded）状態のスケジューラを silent success として扱ってはならない。
  - macOS において Desktop 所有スケジューラが存在する場合、以下の 3 状態に厳密に分類する：
    - **A. Healthy**: Desktop owner + loaded/ready。CLI からの `schedule` はそのまま成功、所有権変更なし。
    - **B. Unloaded / Recoverable**: Desktop owner + valid plist/executable + not loaded。CLI は Desktop の plist 内容およびバイナリパスを変更せず、そのまま `launchctl load -w` による safe reload / repair を試行する。repair 成功時は ready=true でスケジュール続行。repair 失敗時は構造化エラーを返し、ジョブを JobStore に保存しない。
    - **C. Malformed / Invalid / Unknown**: 解析不能またはバイナリ消失。既存の非破壊ポリシーを維持し、CLI 自身のバイナリで勝手に置換せず、構造化エラーを返却する。
  - **スケジュールのアトミック性（Schedule Atomic Behavior）**:
    - CLI `schedule` では「健全性確認 → 必要なら safe repair → ready 確認 → その後 job insert」の順序を厳格に保証する。
    - macOS において常設スケジューラが必要であるにもかかわらず ready にできなかった場合、JobStore へ新規ジョブを残してはならない（MUST NOT save job to store on scheduler readiness failure）。既存ジョブは削除・変更しない。
  - **`install-scheduler` 挙動の是正**:
    - Desktop owner + ready: `status: "retained"`, healthy。
    - Desktop owner + not ready: safe reload / repair を試行し、成功なら `status: "repaired"`, ready=true。失敗なら非ゼロ終了（構造化エラー）。Desktop ownership を CLI へ移行してはならない。
  - **`status --json` セマンティクスの自動化安全化（Automation-Safe）**:
    - 公開済みフィールドの削除は breaking になるため行わない（`path_matched` を維持）。
    - `path_matched` の意味を「呼び出し元実行ファイル（caller exe）と登録実行ファイル（registered exe）の一致」として仕様に明記。
    - 加算（additive）フィールドとして以下を追加：
      - `target_exists`: bool（登録実行ファイルがディスク上に実在するか）
      - `owner_target_valid`: bool（登録実行ファイルが検出された所有者種別の正当なバイナリ形式であるか）
    - 自動化スクリプトが「owner=desktop かつ ready=true かつ target_exists=true かつ owner_target_valid=true」により、`path_matched=false` であっても健全と判定できる仕様とする。
- **Agent Decision**:
  - `MacOsLaunchdScheduler` に `repair_desktop_scheduler()` および `is_target_executable_valid()` 判定ロジックを追加し、`ensure_scheduler_installed` の Desktop owner 分岐において未ロード時の再ロード処理を透過的に統合する。
  - `SchedulerService::schedule_job` では、`ensure_scheduler_installed` 実行後に `is_scheduler_ready()` を再検証し、未準備であれば JobStore への登録を行わずにエラーを返す（アトミック性）。

## 確定した決定事項（Confirmed Decisions）

1. **Desktop 所有権の不可侵・非破壊修復**:
   - CLI は Desktop 所有 LaunchAgent の plist 内容（Desktop アプリ実行ファイルパス、引数等）を変更してはならない（MUST NOT modify content）。
   - 修復は既存 plist パスに対する `launchctl load -w <plist>` の実行のみによって行われる（SHOULD）。
2. **新規スケジュールのアトミック保証**:
   - macOS において、スケジューラが最終的に ready にならなかった場合、ジョブを「成功」として JobStore に書き込んではならない（MUST NOT persist due job on scheduler readiness failure）。
3. **`status --json` の完全な後方互換性**:
   - v0.4.0 で定義された 5 フィールド（`installed`, `ready`, `owner`, `executable`, `path_matched`）をすべて維持し、新フィールド（`target_exists`, `owner_target_valid`）をオプショナルまたはブール値として加算する。

## 変更・追加される規範要件（New / Changed Requirements）

### CLI 仕様 (`docs/specs/cli.md`)

#### CLI-CMD-003: 機械可読 JSON 出力モード（改定）
- `status --json` の `scheduler` オブジェクトに以下を含めなければならない（MUST）：
  - `installed`: bool（登録ファイルの有無）
  - `ready`: bool（OSスケジューラ loaded 状態）
  - `owner`: `"desktop" | "cli" | "none" | "legacy" | "invalid"`
  - `executable`: 登録実行ファイルパス（Option<String>）
  - `path_matched`: bool（呼び出し元プロセス実行ファイルと登録実行ファイルの一致）
  - `target_exists`: bool（登録実行ファイルがファイルシステム上に実在するか）
  - `owner_target_valid`: bool（登録実行ファイルが検出所有者の正当なバイナリであるか）
- **自動化健全性判定規則**:
  - 自動化スクリプトまたは外部エージェントは、`installed == true && ready == true && target_exists == true && owner_target_valid == true` をもってスケジューラが正常稼働可能であると判定しなければならない（MUST）。
  - `owner == "desktop"` の場合、CLI からの呼出において `path_matched == false` となることは正常かつ期待される動作であり、これを異常（failure）と判定してはならない（MUST NOT）。

#### CLI-CMD-004: スケジューラ所有権認識と健全性検証・修復（改定）
- **Desktop 所有スケジューラの健全性検証と修復**:
  - macOS 環境において Desktop 所有のスケジューラが登録されている場合、CLI はその稼働状態（ready）を検査しなければならない（MUST）。
  - 未ロード（not loaded / ready=false）の場合、CLI は Desktop 所有 LaunchAgent plist の内容を変更することなく、`launchctl load -w` による安全な再ロード（safe repair）を試行しなければならない（MUST）。
  - 修復が成功した場合、所有権は `Desktop` のまま維持され、`ready == true` とならなければならない（MUST）。
  - 修復が失敗した場合、または plist 自体が破損・バイナリ消失（invalid）している場合、CLI はエラーを返さなければならず、Desktop plist を自身の CLI バイナリで勝手に上書きしてはならない（MUST NOT overwrite with CLI binary）。
- **新規スケジュールの保存アトミック性**:
  - macOS 環境において、スケジューラが正常に ready 状態とならなかった場合、`schedule` コマンドはジョブを JobStore に保存してはならず（MUST NOT）、構造化エラー（`schedule_failed`）を出力して非ゼロ終了しなければならない（MUST）。
- **`install-scheduler` の修復対応**:
  - Desktop 所有かつ `ready == true` の場合: `status: "retained"`, 所有権を維持。
  - Desktop 所有かつ `ready == false` の場合: safe repair を試行し、成功時は `status: "repaired"`、失敗時は非ゼロ終了および構造化エラーを出力（MUST）。

### OS スケジューラ仕様 (`docs/specs/os-scheduler.md`)

#### OS-SCHED-006: macOS スケジューラ所有権モデルと優先度（改定）
- **Desktop 所有スケジューラの健全性状態モデル**:
  - `Healthy`: Desktop 所有かつ `launchctl list` にロード済（`is_scheduler_ready() == true`）。
  - `Unloaded / Recoverable`: Desktop 所有かつ登録 plist および実行ファイルが実在するが、`launchctl list` に未ロード（`is_scheduler_ready() == false`）。
  - `Malformed / Invalid`: 登録 plist の構文不正、未対応ラベル、または登録実行ファイルがディスク上に存在しない状態。
- **修復規則（Repair Rules）**:
  - `Unloaded / Recoverable` 状態において、CLI または Desktop からの ensure / repair 要求は、plist ファイルを再生成・改変することなく、既存 plist をそのまま `launchctl load -w` でロードする（MUST）。
  - CLI が Desktop 所有 LaunchAgent を自身のバイナリへ書き換えて乗っ取ることは禁止される（MUST NOT）。
- **スケジュールアトミック性（Schedule Atomicity Invariant）**:
  - macOS において、常設スケジューラによる定期 tick 実行が前提となるジョブ登録処理は、スケジューラが ready であることを確認した後にのみ JobStore への書き込みを行わなければならない（MUST）。

## 互換性への影響（Compatibility Impact）

- **後方互換性（Backward Compatibility）**:
  - `status --json` に新しいフィールド（`target_exists`, `owner_target_valid`）が追加されるが、既存の 5 フィールド（`installed`, `ready`, `owner`, `executable`, `path_matched`）はすべて維持されるため、既存の JSON パーサーを破壊しない。
  - `schedule --json`, `list --json`, `show --json` 等の入出力スキーマに変更はない。
  - Desktop GUI の Diagnostics UI との整合性も完全に維持される。
- **破壊的変更（Breaking Changes）**: なし。

## 実装への影響（Implementation Impact）

- `crates/codex-scheduler-core/src/os_scheduler/macos.rs`:
  - `repair_desktop_scheduler()` または `ensure_scheduler_installed()` 内での Desktop owner かつ not loaded 時の `launchctl load -w` 実行。
  - ターゲット実行ファイル存在・妥当性判定ヘルパーの追加。
- `crates/codex-scheduler-core/src/os_scheduler/mod.rs`:
  - `SchedulerBackend` トレイトへの `is_target_executable_exists()`, `is_owner_target_valid()` 等のヘルパーメソッド追加（デフォルト実装あり）。
- `crates/codex-scheduler-core/src/lib.rs`:
  - `SchedulerService::schedule_job()` における readiness 最終確認と未達成時の早期エラー復帰（JobStore 書き込み防止）。
- `crates/codex-scheduler-cli/src/main.rs`:
  - `SchedulerStatusOutput` への新フィールド追加。
  - `install-scheduler` における Desktop owner not ready 時の repair 試行とステータス分岐。

## 承認適格性（Approval Eligibility）

- **Autonomous approval eligible**: Yes
- **Human gate**: None
- **Rationale**:
  - Human が本 Objective を明示的に選択し、要件定義および動作方針（3状態モデル、repair方針、scheduleアトミック性、status互換性）を指定済み。
  - 既存の Approved 決定（Desktop 優先、単一 LaunchAgent、CLI は Desktop を奪わない）と完全に整合。
  - 互換性破壊や不可逆なデータ変更を伴わない。

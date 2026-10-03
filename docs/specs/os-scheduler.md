# 仕様: OSスケジューラ連携（OS Scheduler Integration）

Status: Approved

Domain: OSSCHED

## 概要

GUIデスクトップアプリが終了・就寝中であっても、指定時刻以降にOSネイティブのタイマー機能（定期ポーリング）によってバックグラウンドで待機中ジョブを実行するための仕様を定める。
現行バージョン（v0.4.0）における常設OSスケジューラ（バックグラウンド自動定期実行）の production backend は **macOS (LaunchAgent)** のみである。
Windows 環境においては、常設 Task Scheduler 連携は計画仕様（次期 Objective 候補）として位置付けられ、現行コア実装は `FallbackScheduler`（内部 no-op、スケジューラ未登録・未準備状態）として動作する。Windows ではアプリ起動中の管理・手動実行のみがサポートされ、アプリ終了後の自動起動はサポートされない。
macOS においてはジョブごとに新しいバックグラウンド項目通知が出るのを防止するため、アプリ全体で1つの固定LaunchAgentを常設し、アプリ内部で複数ジョブを管理するアーキテクチャを採用する（1 Application = 1 LaunchAgent, N Jobs = jobs store内部管理）。
また、Gatekeeperによる別バイナリ拒否を根本排除するため、macOSデスクトップ環境ではメインアプリ実行ファイル（`Codex Scheduler.app/Contents/MacOS/...`）自身をヘッドレスモード（`--scheduler-tick`）で起動する「1 app / 1 executable / 2 execution modes」構成とする。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### OS-SCHED-001: macOS launchd 単一常設連携（通知最小化とシングルバイナリ実行）

macOS環境において、バックグラウンド実行を担保するために以下の仕様に従ってアプリ専用の単一LaunchAgentを登録・維持しなければならない（MUST）。

1. **単一常設Plistファイル**:
   - パス: `~/Library/LaunchAgents/dev.codexscheduler.scheduler.plist`
   - ラベル: `dev.codexscheduler.scheduler`
   - `ProgramArguments`: `[<app_executable_path>, "--scheduler-tick"]`
   - 実行ファイルパス取得: `/Applications/...` をハードコードしてはならず（MUST NOT）、`std::env::current_exe()` 等を用いて現在実行中のアプリ本体の絶対パスを動的に取得しなければならない（MUST）。
   - `StartInterval`: `60`（60秒間隔で定期的に待機中ジョブを確認・実行）
   - `RunAtLoad`: `true`（登録時およびログイン時に即時チェック）
   - `AbandonProcessGroup`: `true`
   - `StandardOutPath`: `/tmp/dev.codexscheduler.scheduler.stdout.log`
   - `StandardErrorPath`: `/tmp/dev.codexscheduler.scheduler.stderr.log`
2. **初回登録・パス更新・冪等性**:
   - 既存のplistが存在する場合、その `ProgramArguments` 内の実行ファイルパスと現在のアプリ実行ファイルパスを比較する。
   - パスが同一かつ既に `launchctl` にロード済みであれば、再登録を行ってはならない（MUST NOT）。これによりジョブ作成ごとのバックグラウンド通知の再発を防止する。
   - アプリの移動や更新により実行ファイルパスが変更された場合、または初回登録時は、安全にアンロード・書き込み・再ロードを行わなければならない（MUST）。
3. **登録エラーの厳格な伝播（握りつぶし禁止）**:
   - plistディレクトリ作成失敗、plist書き込み失敗、`launchctl load/unload` の失敗、実行ファイル不存在等は、すべて structured error（`SchedulerError`）として返し、握りつぶして成功扱いにしてはならない（MUST NOT）。GUI側へエラーを伝播させ、スケジューラが正常登録されていない状態でジョブが安全にスケジュールされたと誤認させてはならない（MUST NOT）。
4. **レガシーPlistのクリーンアップ**:
   - 単一常設LaunchAgentのプロビジョニング時に、過去の個別ジョブplist（`com.codexscheduler.job.*.plist`）が存在する場合は自動的に `launchctl unload` およびファイル削除を行わなければならない（MUST）。
   - 過去のplistが外部Worker CLI（`codex-scheduler-cli`）を指している場合も、新しいメインアプリ実行ファイルへ自動更新しなければならない（MUST）。
5. **ジョブの登録・編集・削除**:
   - ジョブの追加、編集、削除、リトライ時刻更新はすべて `jobs.json` の更新のみで完結し、LaunchAgentの追加・更新・削除は行わない（MUST NOT）。

### OS-SCHED-002: Windows Task Scheduler 連携（計画仕様 / 次期Objective候補）

※ 現行バージョン（v0.4.0）のコア実装において、non-macOS プラットフォームは `FallbackScheduler`（内部no-op）として動作し、Windows Task Scheduler への常設タスク自動登録は未実装である。
コア API は `is_scheduler_installed() == false`, `is_scheduler_ready() == false`, `is_scheduler_path_matched() == false`, `get_scheduler_owner() == SchedulerOwner::None` を返し、存在しない常設スケジューラを Ready と誤認させない。
Windows Task Scheduler backend の本格統合は次期 Objective 候補とし、本仕様は将来の実装要件として定義する。

Windows環境において、タスクスケジューラ連携実装時は以下の仕様に従ってタスクを登録・管理しなければならない（MUST）。

1. **タスク名**: `CodexScheduler_Service`（または `CodexScheduler_<job_id>`）
2. **登録・実行**:
   - 定期タスク（1分間隔）として `codex-scheduler tick` を登録するか、または個別タスクとして起動。
3. **クリーンアップ**:
   - ジョブ完了時またはアンインストール時に適切にタスクを整理する。

### OS-SCHED-003: ヘッドレスモード要件と共有コアセマンティクス

1. **ヘッドレスモード要件（`--scheduler-tick`）**:
   - LaunchAgent から起動された場合、以下の要件をすべて満たさなければならない（MUST）：
     - Tauri GUI window を生成しない。
     - Dock へ通常 GUI として表示しない（LSUIElement / background process 相当）。
     - Webview / frontend を初期化しない。
     - ダイアログプラグイン等の GUI 依存機能を初期化しない。
     - GUI process を起動したような副作用を発生させない。
     - 共有コアロジック（`codex-scheduler-core`）を用いて待機中ジョブのみを実行する。
     - 実行対象のジョブが存在しない場合、数ミリ秒で直ちに正常終了（exit code 0）する。
2. **多重実行防止（Atomic Claim）**:
   - 60秒間隔の定期 tick が前回の完了前に重なった場合や、複数プロセスが同時に起動した場合でも、同一ジョブが二重実行されてはならない（MUST NOT）。
   - コアのジョブストアは、待機中ジョブ（`Scheduled` または `Retrying`）の抽出とステータスから `Running` への更新をアトミックに排他制御しなければならない（MUST）。
3. **実行セマンティクスの共有**:
   - ヘッドレスモードでのジョブ実行、リトライ判定、履歴保存は、GUI の「今すぐ実行」と完全に同一の `codex-scheduler-core` ロジックを使用しなければならない（MUST）。ロジックを GUI や CLI 側へ個別に複製してはならない（MUST NOT）。

### OS-SCHED-004: アプリ起動時の同期・フォールバック

GUIアプリが起動している間は、OSスケジューラに加え、アプリ内タイマーでも待機ジョブの状態を監視し、時刻が到来して未実行のまま放置されているジョブ（PC電源断などでスキップされたジョブ等）を検知してユーザーに通知または自動リカバリできる（MAY）。

### OS-SCHED-005: 予約時刻セマンティクス（Earliest Execution Time とポーリング遅延）

OSスケジューラ連携におけるジョブ実行予定日時（`scheduled_at`）は、**最早実行開始時刻（Earliest Execution Time）** として定義され、以下の仕様に従わなければならない（MUST）。

1. **非厳格時刻実行（No Exact-Time Guarantee）**:
   - `scheduled_at` は指定時刻ちょうど（秒単位の厳密な一致）での起動を保証するものではない（MUST NOT guarantee exact-time trigger）。
2. **期限到来条件（Due Condition）**:
   - ジョブが実行対象（Due）となる条件は `scheduled_at <= current_time` であり、かつステータスが `Scheduled` または `Retrying` であること（MUST）。
3. **定期ポーリング起動**:
   - macOS `launchd`（`StartInterval: 60`）等の定期ポーリング tick により、期限到来したジョブが検出・抽出（claim）される（Windows では将来の Task Scheduler 連携またはアプリ起動中タイマー / 手動 tick により検出）。
   - 期限到来したジョブは、到来時刻以降に最初に到来するスケジューラ tick において実行対象となる（MUST）。
4. **通常遅延と追加遅延**:
   - 定期ポーリング間隔が60秒であるため、通常運用時における実行開始は `scheduled_at` 到来後 0〜約60秒以内となる（SHOULD）。
   - ただし、OSのスリープ・復帰タイミング、システム高負荷、スケジューラプロセスのキューイング遅延等により、追加の遅延が発生することが許容される（MAY）。

### OS-SCHED-006: macOS スケジューラ所有権モデルと優先度（Single Scheduler Ownership & Precedence）

macOS環境において、単一常設 LaunchAgent（`dev.codexscheduler.scheduler`）の所有権（Ownership）は以下の規則に従って管理されなければならない（MUST）。

1. **単一 LaunchAgent 不変条件**:
   - システム内に登録される LaunchAgent は常に `dev.codexscheduler.scheduler` の1つのみでなければならず、複数作成してはならない（MUST NOT）。
2. **所有権の分類（Scheduler Owner）**:
   - `Desktop`: `ProgramArguments[0]` が有効な Desktop GUI アプリケーション（`.app` 内の実行ファイルまたは `codex-scheduler-gui`）を指している状態。
   - `Cli`: `ProgramArguments[0]` が有効な standalone CLI 実行ファイル（`codex-scheduler` または `codex-scheduler-cli`）を指している状態。
   - `None`: LaunchAgent plist が存在しない状態。
   - `Legacy`: 過去バージョンの引数形式（旧 Worker 呼出等）や個別ジョブ plist が残存している状態。
   - `Invalid`: 登録ファイルが存在するが構文が不正、または登録された実行ファイルがディスク上に存在しない状態。
3. **優先度ルール（Precedence Rules）**:
   - **Desktop 優先（Desktop Precedence）**: 有効な `Desktop` 所有の登録が存在する場合、CLI からのスケジューラ登録（ensure）は既存の LaunchAgent を上書きしてはならない（MUST NOT overwrite）。CLI は Desktop 所有スケジューラをそのまま維持し、ジョブ登録のみを行う。
   - **CLI 所有からの安全な移行（Safe Migration）**: `Cli` 所有の状態で Desktop GUI が起動された場合、Desktop は LaunchAgent の実行主体をメインアプリ実行ファイルへ安全に更新・移行（takeover）してよい（MAY）。
   - **消失した stale 登録の修復（Stale Repair）**: 登録先バイナリが存在しない既知の stale 登録（`Invalid`）は、利用可能な frontend（Desktop または CLI）が安全に自己の実行ファイルで上書き・修復できる（MAY）。
   - **未知・破損設定の保護（Malformed Protection）**: 解析不能な未知の設定や手動破損ファイルは、デフォルトで無言上書きしてはならず（MUST NOT）、構造化されたエラーとして報告しなければならない（MUST）。
   - **アンインストール保護（Uninstall Protection）**: CLI の `uninstall-scheduler` コマンドは、現在の所有者が `Desktop` である場合はアンインストールを実行してはならず（MUST NOT）、エラーを返して Desktop スケジューラを保護しなければならない（MUST）。

## 検証ルール

- 単一常設plistファイルが正しい構文（XML）および `StartInterval: 60` で出力されることをユニットテストで検証する。
- 登録処理が冪等であり、既存登録時に重複してコマンド実行されないことを検証する。
- レガシーplist（`com.codexscheduler.job.*.plist`）の検出および削除処理を検証する。
- `tick` コマンドで期限到来ジョブ（Scheduled / Retrying）が正しく実行され、期限未到来ジョブおよび終端ステータスがスキップされることを検証する。
- 所有権判定（Desktop / Cli / None / Legacy / Invalid）が正しく機能することを検証する。
- Desktop 所有時に CLI の `ensure` が上書きせず維持されること、および `uninstall` が保護エラーとなることを検証する。
- CLI 所有時に Desktop の `ensure` が安全に Desktop 所有へ移行することを検証する。

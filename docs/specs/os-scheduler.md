# 仕様: OSスケジューラ連携（OS Scheduler Integration）

Status: Approved

Domain: OSSCHED

## 概要

GUIデスクトップアプリやCLIターミナルが終了・就寝中であっても、指定時刻以降にOSネイティブのタイマー機能（定期ポーリング）によってバックグラウンドで待機中ジョブを実行するための仕様を定める。
OSスケジューラ連携の production backend として、**macOS (LaunchAgent)** および **Windows (Task Scheduler)** をサポートする。
いずれのOSにおいても、ジョブごとにOSスケジューラ登録を乱立させるのを防ぎ、単一の常設スケジューラが共有ジョブストア（`jobs.json`）を定期ポーリング（60秒間隔）して期限到来ジョブを自動実行するアーキテクチャを採用する（1 Application = 1 persistent scheduler per OS/user, N Jobs = jobs.json内部管理）。
また、別バイナリ起動による権限・セキュリティ問題を排除するため、Desktop環境ではメインアプリ実行ファイル（macOS: `Codex Scheduler.app/.../codex-scheduler-gui`、Windows: `codex-scheduler-gui.exe`）自身をヘッドレスモード（`--scheduler-tick`）で起動する「1 app / 1 executable / 2 execution modes」構成とする。standalone CLI環境ではCLI実行ファイル（`codex-scheduler` / `codex-scheduler.exe`）自身が同様に `--scheduler-tick` を実行する。

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

### OS-SCHED-002: Windows Task Scheduler 単一常設連携（Production Backend）

Windows環境において、バックグラウンド定期実行を担保するために以下の仕様に従って単一の常設タスク（`CodexScheduler_Service`）を登録・管理しなければならない（MUST）。

1. **単一タスク識別と不変条件**:
   - タスク名: `CodexScheduler_Service`
   - システム内に登録されるタスクは常に `CodexScheduler_Service` の1つのみであり、ジョブごとの個別タスクを作成してはならない（MUST NOT）。
2. **正規タスク構成（Canonical Task Definition）**:
   - **Trigger**: 1分間隔の反復実行（`Repetition.Interval: PT1M`）、常設リピート、`Enabled: true`。
   - **Action**: 実行ファイル絶対パスおよび引数 `--scheduler-tick`。
   - **Security**: Current User context（管理者特権・SYSTEM実行・パスワード保存を要求せず、UAC昇格を求めない `InteractiveToken`）。
   - **Settings**:
     - `MultipleInstancesPolicy: IgnoreNew`（重複実行防止）
     - `DisallowStartIfOnBatteries: false`（バッテリー駆動時も実行）
     - `StopIfGoingOnBatteries: false`
     - `RunOnlyIfIdle: false`
     - `RunOnlyIfNetworkAvailable: false`
     - `StartWhenAvailable: true`（予定時刻経過後の再開時即時実行）
     - `WakeToRun: false`（スリープ解除は強制せず、復帰後次tickで処理）
3. **OS境界と Task Scheduler 2.0 COM API 実行契約**:
   - Windows Task Scheduler 2.0 COM API（`ITaskService`, `ITaskFolder`, `IRegisteredTask`）を使用し、カレントユーザーのインタラクティブトークン（`TASK_LOGON_INTERACTIVE_TOKEN`）および最低特権（`TASK_RUNLEVEL_LUA` / `LeastPrivilege`）で操作する。
   - 一般ユーザー（非管理者 / Standard User）環境で管理者昇格（UAC elevation）を一切要求せず、タスクの登録・更新・照会・削除を完結させる（`schtasks.exe` コマンドラインツールはローカルタスク作成・更新時に管理者特権を要求するため使用しない）。
   - タスクの存在確認および構成照会は COM API のプロパティおよび定義 XML を構造的に解析する。
   - テスト容易性のための抽象化境界（`TaskSchedulerRunner` トレイト）を介して実行し、ユニットテストで実機の Scheduled Task を汚染・破壊しない。
4. **所有権モデルと優先度ルール**:
   - `Desktop`: アクション実行ファイル名が `codex-scheduler-gui.exe` であり実在する。
   - `Cli`: アクション実行ファイル名が `codex-scheduler.exe`（またはlegacy互換名 `codex-scheduler-cli.exe`）であり実在する。
   - `None`: `CodexScheduler_Service` が存在しない。
   - `Invalid`: アクション不存在、引数不正、実行ファイル消失（stale）、または構文破損。
   - **Desktop 優先（Desktop Precedence）**: 有効な Desktop 所有タスクが存在する場合、CLI はタスクを変更せず維持する（MUST NOT overwrite）。未ロード・無効化状態（ready == false）の場合は同一 Desktop ターゲットを維持して修復（safe repair）する。
   - **CLI から Desktop への安全な移行（Safe Takeover）**: CLI 所有タスクが存在する状態で Desktop GUI が起動された場合、Desktop はタスクの実行主体を `codex-scheduler-gui.exe` へ安全に移行する（MAY）。
   - **Invalid / Stale 登録の非破壊保護**: ターゲット実行ファイルが消失した stale 登録に対し、CLI は自身のバイナリで上書きしてはならず（MUST NOT）、構造化エラー（`ExecutableNotFound`）を返す。未知の構文破損タスクは上書きせず `MalformedConfiguration` を返す。
5. **スケジュールアトミック性（Schedule Atomicity Invariant）**:
   - スケジューラが ready であることを確認した後にのみ新規ジョブを JobStore（`jobs.json`）へ保存する。修復失敗や破損時は保存を遮断しエラーを返す。

### OS-SCHED-003: ヘッドレスモード要件と共有コアセマンティクス

1. **ヘッドレスモード要件（`--scheduler-tick`）**:
   - LaunchAgent または Task Scheduler から起動された tick プロセスは、以下の要件をすべて満たさなければならない（MUST）：
     - Tauri GUI window を生成しない。
     - Dock へ通常 GUI として表示しない（LSUIElement / background process 相当）。
     - Webview / frontend を初期化しない。
     - ダイアログプラグイン等の GUI 依存機能を初期化しない。
     - GUI process を起動したような副作用を発生させない。
     - 共有コアロジック（`codex-scheduler-core`）を用いて待機中ジョブのみを抽出（claim）し、独立したRunnerプロセスを安全に起動（detached spawn）する。
     - 長時間継続するCodexプロセスの完了をtick自身が同期的に待機（await）してはならず（MUST NOT）、Runner起動完了後、数ミリ秒〜数十ミリ秒以内に直ちに正常終了（exit code 0）する。
     - 実行対象のジョブが存在しない場合も、数ミリ秒で直ちに正常終了（exit code 0）する。
2. **多重実行防止（Atomic Claim）と同一Session排他**:
   - 60秒間隔の定期 tick が前回の完了前に重なった場合や、複数プロセスが同時に起動した場合でも、同一ジョブが二重実行されてはならない（MUST NOT）。
   - コアのジョブストアは、待機中ジョブ（`Scheduled` または `Retrying`）の抽出とステータスから `Running` への更新をアトミックに排他制御しなければならない（MUST）。
   - 同一の `session_id` を持つ別のジョブが既に `Running` 状態である場合、後続の同一セッション向けジョブのclaimは先行ジョブの完了まで保留（スキップ）し、同一Codexセッションに対する二重writer競合を防止しなければならない（MUST）。
3. **実行セマンティクスの共有**:
   - Desktop所有時（`codex-scheduler-gui --run-job <job_id>`）およびCLI所有時（`codex-scheduler run-job <job_id>`）のいずれにおいても、Runnerプロセスでのジョブ実行、リトライ判定、履歴保存は、完全に同一の `codex-scheduler-core` ロジックを使用しなければならない（MUST）。ロジックを GUI や CLI 側へ個別に複製してはならない（MUST NOT）。

### OS-SCHED-004: アプリ起動時の同期・フォールバック

GUIアプリが起動している間は、OSスケジューラに加え、アプリ内タイマーでも待機ジョブの状態を監視し、時刻が到来して未実行のまま放置されているジョブ（PC電源断などでスキップされたジョブ等）を検知してユーザーに通知または自動リカバリできる（MAY）。

### OS-SCHED-005: 予約時刻セマンティクス（Earliest Execution Time とポーリング遅延）

OSスケジューラ連携におけるジョブ実行予定日時（`scheduled_at`）は、**最早実行開始時刻（Earliest Execution Time）** として定義され、以下の仕様に従わなければならない（MUST）。

1. **非厳格時刻実行（No Exact-Time Guarantee）**:
   - `scheduled_at` は指定時刻ちょうど（秒単位の厳密な一致）での起動を保証するものではない（MUST NOT guarantee exact-time trigger）。
2. **期限到来条件（Due Condition）**:
   - ジョブが実行対象（Due）となる条件は `scheduled_at <= current_time` であり、かつステータスが `Scheduled` または `Retrying` であること（MUST）。
3. **定期ポーリング起動**:
   - macOS `launchd`（`StartInterval: 60`）または Windows Task Scheduler（1分間隔反復トリガー）の定期ポーリング tick により、期限到来したジョブが検出・抽出（claim）される。
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
3. **Desktop 所有スケジューラの健全性状態モデル**:
   - `Healthy`: Desktop 所有かつ `launchctl list` に正常登録・稼働中（`is_scheduler_ready() == true`）。
   - `Unloaded / Recoverable`: Desktop 所有かつ登録 plist および実行ファイルが実在するが、`launchctl list` に未ロード（`is_scheduler_ready() == false`）。
   - `Malformed / Invalid`: 登録 plist の構文不正、未対応ラベル、または登録実行ファイルがディスク上に存在しない状態。
4. **優先度および修復ルール（Precedence & Repair Rules）**:
   - **Desktop 優先（Desktop Precedence）**: 有効な `Desktop` 所有の登録が存在する場合、CLI からのスケジューラ登録（ensure）は既存の LaunchAgent を上書きしてはならない（MUST NOT overwrite）。CLI は Desktop 所有スケジューラをそのまま維持し、ジョブ登録のみを行う。
   - **未ロード Desktop スケジューラの安全修復（Safe Reload Repair）**: `Unloaded / Recoverable` 状態の Desktop 所有スケジューラが存在する場合、CLI または Desktop からの ensure / repair 要求は、plist ファイル内容（実行ファイルパスや引数）を変更することなく、既存 plist をそのまま `launchctl load -w` で再ロード（safe repair）しなければならない（MUST）。CLI バイナリによる上書きや所有権の奪取を行ってはならない（MUST NOT overwrite with CLI binary）。
   - **CLI 所有からの安全な移行（Safe Migration）**: `Cli` 所有の状態で Desktop GUI が起動された場合、Desktop は LaunchAgent の実行主体をメインアプリ実行ファイルへ安全に更新・移行（takeover）してよい（MAY）。
   - **消失した stale 登録の修復（Stale Repair）**: 登録先バイナリが存在しない既知の stale 登録（`Invalid`）は、利用可能な frontend（Desktop または CLI）が安全に自己の実行ファイルで上書き・修復できる（MAY）。
   - **未知・破損設定の保護（Malformed Protection）**: 解析不能な未知の設定や手動破損ファイルは、デフォルトで無言上書きしてはならず（MUST NOT）、構造化されたエラーとして報告しなければならない（MUST）。
   - **アンインストール保護（Uninstall Protection）**: CLI の `uninstall-scheduler` コマンドは、現在の所有者が `Desktop` である場合はアンインストールを実行してはならず（MUST NOT）、エラーを返して Desktop スケジューラを保護しなければならない（MUST）。
5. **スケジュールアトミック性（Schedule Atomicity Invariant）**:
   - macOS において、常設スケジューラによる定期 tick 実行が前提となるジョブ登録処理は、スケジューラが ready であることを確認した後にのみ JobStore への書き込みを行わなければならない（MUST）。
   - safe repair に失敗した場合や設定破損により ready にできなかった場合、新規ジョブを「成功」として JobStore に保存してはならず（MUST NOT persist due job on scheduler readiness failure）、構造化エラーを返さなければならない（MUST）。

## 検証ルール

- 単一常設plistファイルが正しい構文（XML）および `StartInterval: 60` で出力されることをユニットテストで検証する。
- 登録処理が冪等であり、既存登録時に重複してコマンド実行されないことを検証する。
- レガシーplist（`com.codexscheduler.job.*.plist`）の検出および削除処理を検証する。
- `tick` コマンドで期限到来ジョブ（Scheduled / Retrying）が正しく実行され、期限未到来ジョブおよび終端ステータスがスキップされることを検証する。
- 所有権判定（Desktop / Cli / None / Legacy / Invalid）が正しく機能することを検証する。
- Desktop 所有時に CLI の `ensure` が上書きせず維持されること、未ロード時に既存 plist を維持して再ロード修復されること、および `uninstall` が保護エラーとなることを検証する。
- CLI 所有時に Desktop の `ensure` が安全に Desktop 所有へ移行することを検証する。
- スケジューラ修復失敗時に新規ジョブが JobStore に残らないことを検証する。

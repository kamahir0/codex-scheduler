# 仕様: OSスケジューラ連携（OS Scheduler Integration）

Status: Approved

Domain: OSSCHED

## 概要

GUIデスクトップアプリが終了・就寝中であっても、指定時刻にOSネイティブのタイマー機能によってバックグラウンドで待機中ジョブを実行するための仕様を定める。
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

### OS-SCHED-002: Windows Task Scheduler 連携

Windows環境において、ジョブ登録時は以下の仕様に従ってタスクを登録・管理しなければならない（MUST）。

1. **タスク名**: `CodexScheduler_Service`（または `CodexScheduler_<job_id>`）
2. **登録・実行**:
   - 定期タスク（1分間隔）として `codex-scheduler-cli tick` を登録するか、または個別タスクとして起動。
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
   - コアのジョブストアは、待機中ジョブの抽出とステータス `Scheduled -> Running` への更新をアトミックに排他制御しなければならない（MUST）。
3. **実行セマンティクスの共有**:
   - ヘッドレスモードでのジョブ実行、リトライ判定、履歴保存は、GUI の「今すぐ実行」と完全に同一の `codex-scheduler-core` ロジックを使用しなければならない（MUST）。ロジックを GUI や CLI 側へ個別に複製してはならない（MUST NOT）。

### OS-SCHED-004: アプリ起動時の同期・フォールバック

GUIアプリが起動している間は、OSスケジューラに加え、アプリ内タイマーでも待機ジョブの状態を監視し、時刻が到来して未実行のまま放置されているジョブ（PC電源断などでスキップされたジョブ等）を検知してユーザーに通知または自動リカバリできる（MAY）。

## 検証ルール

- 単一常設plistファイルが正しい構文（XML）および `StartInterval: 60` で出力されることをユニットテストで検証する。
- 登録処理が冪等であり、既存登録時に重複してコマンド実行されないことを検証する。
- レガシーplist（`com.codexscheduler.job.*.plist`）の検出および削除処理を検証する。
- `tick` コマンドで期限到来ジョブが正しく実行され、期限未到来ジョブがスキップされることを検証する。

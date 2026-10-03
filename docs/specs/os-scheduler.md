# 仕様: OSスケジューラ連携（OS Scheduler Integration）

Status: Approved

Domain: OSSCHED

## 概要

GUIデスクトップアプリが終了・就寝中であっても、指定時刻にOSネイティブのタイマー機能によってバックグラウンド worker プロセス（`codex-scheduler-cli`）を起動するための仕様を定める。
macOS においてはジョブごとに新しいバックグラウンド項目通知が出るのを防止するため、アプリ全体で1つの固定LaunchAgentを常設し、Worker自身が複数ジョブを管理するアーキテクチャを採用する（1 Application = 1 LaunchAgent, N Jobs = Worker内部管理）。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### OS-SCHED-001: macOS launchd 単一常設連携（通知最小化）

macOS環境において、バックグラウンド実行を担保するために以下の仕様に従ってアプリ専用の単一LaunchAgentを登録・維持しなければならない（MUST）。

1. **単一常設Plistファイル**:
   - パス: `~/Library/LaunchAgents/dev.codexscheduler.scheduler.plist`
   - ラベル: `dev.codexscheduler.scheduler`
   - `ProgramArguments`: `[<canonical_cli_path>, "tick"]`
   - `StartInterval`: `60`（60秒間隔で定期的に待機中ジョブを確認・実行）
   - `RunAtLoad`: `true`（登録時およびログイン時に即時チェック）
   - `AbandonProcessGroup`: `true`
   - `StandardOutPath`: `/tmp/dev.codexscheduler.scheduler.stdout.log`
   - `StandardErrorPath`: `/tmp/dev.codexscheduler.scheduler.stderr.log`
2. **初回登録と冪等性**:
   - 初回プロビジョニング時、またはジョブ登録時に未登録である場合のみ `launchctl load -w ...` を実行する。
   - 既に同内容のplistが登録済みである場合は再ロードを行ってはならない（MUST NOT）。これにより、ジョブ作成・変更・削除のたびにmacOSの「バックグラウンド項目が追加されました」通知が発生するのを完全に防止する。
3. **レガシージョブPlistのクリーンアップ**:
   - 単一常設LaunchAgentのプロビジョニング時に、過去の個別ジョブplist（`com.codexscheduler.job.*.plist`）が存在する場合は自動的に `launchctl unload` およびファイル削除を行わなければならない（MUST）。
4. **ジョブの登録・編集・削除**:
   - ジョブの追加、編集、削除、リトライ時刻更新はすべて `jobs.json` の更新のみで完結し、LaunchAgentの追加・更新・削除は行わない（MUST NOT）。

### OS-SCHED-002: Windows Task Scheduler 連携

Windows環境において、ジョブ登録時は以下の仕様に従ってタスクを登録・管理しなければならない（MUST）。

1. **タスク名**: `CodexScheduler_Service`（または `CodexScheduler_<job_id>`）
2. **登録・実行**:
   - 定期タスク（1分間隔）として `codex-scheduler-cli tick` を登録するか、または個別タスクとして起動。
3. **クリーンアップ**:
   - ジョブ完了時またはアンインストール時に適切にタスクを整理する。

### OS-SCHED-003: Worker プロセスの独立性とTickコマンド

1. **Tick サブコマンド（`codex-scheduler-cli tick`）**:
   - OSスケジューラ（LaunchAgent等）から1分間隔で定期起動される。
   - `jobs.json` をロードし、`status == "scheduled"` かつ `scheduled_at <= Utc::now()` であるジョブを抽出する。
   - 該当するジョブが存在する場合、各ジョブを順次または非同期で `execute_job` 実行する。
   - 実行対象のジョブが存在しない場合、数ミリ秒で直ちに正常終了（exit code 0）し、システムリソースを消費しない（MUST）。
2. **個別の RunJob サブコマンド（`codex-scheduler-cli run-job <job_id>`）**:
   - 手動実行（GUIの「今すぐ実行」）や特定ジョブの直接デバッグ用に引き続き利用可能とする（MUST）。
3. **プロセスの独立性**:
   - Worker プロセスは GUI アプリの稼働状態（起動中・終了中・PCスリープ復帰後）に関係なく完全に独立してジョブを実行し、結果を `jobs.json` に記録する。

### OS-SCHED-004: アプリ起動時の同期・フォールバック

GUIアプリが起動している間は、OSスケジューラに加え、アプリ内タイマーでも待機ジョブの状態を監視し、時刻が到来して未実行のまま放置されているジョブ（PC電源断などでスキップされたジョブ等）を検知してユーザーに通知または自動リカバリできる（MAY）。

## 検証ルール

- 単一常設plistファイルが正しい構文（XML）および `StartInterval: 60` で出力されることをユニットテストで検証する。
- 登録処理が冪等であり、既存登録時に重複してコマンド実行されないことを検証する。
- レガシーplist（`com.codexscheduler.job.*.plist`）の検出および削除処理を検証する。
- `tick` コマンドで期限到来ジョブが正しく実行され、期限未到来ジョブがスキップされることを検証する。

# 仕様: OSスケジューラ連携（OS Scheduler Integration）

Status: Approved

Domain: OSSCHED

## 概要

GUIデスクトップアプリが終了・就寝中であっても、指定時刻にOSネイティブのタイマー機能によってバックグラウンド worker プロセス（`codex-scheduler-cli`）を起動するための仕様を定める。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### OS-SCHED-001: macOS launchd 連携

macOS環境において、ジョブ登録時は以下の仕様に従ってユーザー用LaunchAgentを登録しなければならない（MUST）。

1. **Plistファイル生成**:
   - パス: `~/Library/LaunchAgents/com.codexscheduler.job.<job_id>.plist`
   - ラベル: `com.codexscheduler.job.<job_id>`
   - `ProgramArguments`: `[<cli_path>, "run-job", "<job_id>"]`
   - `StartCalendarInterval`: 指定された日・時・分・秒。
   - `AbandonProcessGroup: true`
2. **ロードと有効化**:
   - `launchctl load ~/Library/LaunchAgents/com.codexscheduler.job.<job_id>.plist` を実行してOSに登録する。
3. **クリーンアップ**:
   - ジョブが完了（`succeeded` / `failed`）またはキャンセル（`cancelled`）された際、workerまたはGUIは `launchctl unload ...` を行い、plistファイルを削除しなければならない（MUST）。

### OS-SCHED-002: Windows Task Scheduler 連携

Windows環境において、ジョブ登録時は以下の仕様に従ってタスクを登録しなければならない（MUST）。

1. **タスク名**: `CodexScheduler_<job_id>`
2. **登録コマンド**:
   - `schtasks.exe /Create /TN "CodexScheduler_<job_id>" /TR "\"<cli_path>\" run-job <job_id>" /SC ONCE /ST <HH:mm> /SD <YYYY/MM/DD> /F`
3. **クリーンアップ**:
   - ジョブ完了時またはキャンセル時、`schtasks.exe /Delete /TN "CodexScheduler_<job_id>" /F` を実行してタスクを削除しなければならない（MUST）。

### OS-SCHED-003: Worker プロセスの独立性

worker プロセス（`codex-scheduler-cli run-job <job_id>`）は、GUIアプリとは完全に独立したプロセスとして起動されなければならない（MUST）。
workerはジョブストア（`~/.codex-scheduler/jobs.json`）を直接読み込み、対象ジョブを実行し、結果をストアに書き戻す。GUIアプリが起動していなくても、終了していても、正常に完遂する。

### OS-SCHED-004: アプリ起動時の同期・フォールバック

GUIアプリが起動している間は、OSスケジューラに加え、アプリ内タイマーでも待機ジョブの状態を監視し、時刻が到来して未実行のまま放置されているジョブ（PC電源断などでスキップされたジョブ等）を検知してユーザーに通知または自動リカバリできる（MAY）。

## 検証ルール

- macOS上でplistファイルが正しい構文（XML）で出力されることをユニットテストで検証する。
- 登録解除処理により、ファイルが確実に削除されることを検証する。

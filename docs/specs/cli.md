# 仕様: コマンドラインインターフェース（Command Line Interface）

Status: Approved

Domain: CLI

## 概要

GUI を必要とせず、ターミナルやリモート開発環境（SSH / エージェント自動化スクリプト等）から Codex の予約実行を管理・自動化するための standalone CLI（`codex-scheduler`）の規範仕様を定める。
CLI はデスクトップ環境への依存を持たず単独で予約・実行が可能であると同時に、Desktop GUI と同一のコア（`codex-scheduler-core`）および共有ジョブストア（`~/.codex-scheduler/jobs.json`）を使用し、完全な相互運用性を提供する。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### CLI-CMD-001: 正式コマンド名および配布形態

1. **正式コマンド名**:
   - ユーザーが実行する正式な CLI コマンド名は `codex-scheduler` でなければならない（MUST）。
   - クレート名（`codex-scheduler-cli`）にかかわらず、公開バイナリ名は `codex-scheduler` とする（Cargo `[[bin]]` 定義）。
2. **独立した実行形態**:
   - CLI は単一の standalone バイナリとして動作し、Desktop GUI アプリケーション（`Codex Scheduler.app` 等）のインストールを必須としてはならない（MUST NOT）。
3. **共有コアの利用**:
   - ジョブモデル、バリデーション、排他制御、リトライ判定、Codex アダプタ呼出、およびスケジューラ tick 実行は、すべて `codex-scheduler-core` のロジックを一元的に使用しなければならない（MUST）。

### CLI-CMD-002: 正式コマンドサーフェス

CLI は最低限以下のサブコマンドを提供しなければならない（MUST）。

1. **`schedule`**: ジョブの新規予約登録
   - `--session-id <id>` (必須): 再開対象の Codex セッション ID。
   - `--cwd <path>` (必須): 実行ディレクトリ。
   - `--prompt <text>` (任意, デフォルト: `"continue"`): 送信するプロンプト。
   - `--at <time>` (必須): 実行予定日時（ISO 8601 / RFC 3339、`+<minutes>`、`YYYY-MM-DD HH:MM` ローカル時刻）。セマンティクスは OS-SCHED-005 に従う最早開始日時（Earliest Execution Time）。
   - `--retry-interval <seconds>` (任意, デフォルト: 300): クォータ制限時のリトライ間隔秒数。
   - `--max-attempts <count>` (任意, デフォルト: 6): 最大試行回数。
2. **`list`**: 登録済み全ジョブの一覧表示
3. **`show <job-id>`**: 指定ジョブの詳細情報および実行履歴の表示
4. **`cancel <job-id>`**: 待機中またはリトライ中のジョブのキャンセル
5. **`delete <job-id>`**: ジョブの削除
6. **`status`**: システム・スケジューラ・所有権情報の表示
7. **`tick`（または `--scheduler-tick`）**: 期限到来ジョブの抽出・実行（内部スケジューラ・テスト用）
8. **`install-scheduler`**: OSスケジューラの常設登録（CLI 所有として登録・修復）
9. **`uninstall-scheduler`**: OSスケジューラの登録解除（所有権配慮型）

### CLI-CMD-003: 機械可読 JSON 出力モード

自動化スクリプトや外部エージェント（Claude Code, Codex 等）からの安全な利用を保証するため、主要コマンドにおいて `--json` オプションを提供しなければならない（MUST）。

1. **成功時の出力**:
   - `--json` が指定された場合、標準出力（stdout）には有効な単一の JSON 文字列のみを出力し、人間向けプロ文や装飾テキストを一切含めてはならない（MUST NOT）。
   - 終了コードは 0（成功）でなければならない（MUST）。
2. **失敗時の出力**:
   - 終了コードは非ゼロ（通常 1）でなければならない（MUST）。
   - 標準出力に中途半端な人間向けテキストを出力してはならず（MUST NOT）、標準エラー出力（stderr）にエラーメッセージを出力するか、構造化された JSON エラーオブジェクトを出力しなければならない（MUST）。
3. **`status --json` スキーマ**:
   - 以下のフィールドを含まなければならない（MUST）：
     - `version`: プロダクトバージョン文字列
     - `store_path`: `jobs.json` の絶対パス
     - `scheduler`:
       - `installed`: bool（登録ファイルの有無）
       - `ready`: bool（正常稼働準備状態）
       - `owner`: `"desktop" | "cli" | "none" | "legacy" | "invalid"`
       - `executable`: 登録実行ファイルパス（Option）
       - `path_matched`: bool
     - `platform`: OS名文字列

### CLI-CMD-004: スケジューラ所有権認識と自己プロビジョニング

1. **CLI-only 環境での自己プロビジョニング**:
   - Desktop が未インストールまたはスケジューラ未登録の環境において、`schedule` または `install-scheduler` 実行時に、CLI 自身を実行主体（`codex-scheduler --scheduler-tick`）とする常設スケジューラを安全に登録（ensure）できなければならない（MUST）。
2. **Desktop 所有スケジューラの尊重**:
   - 既に有効な Desktop 所有のスケジューラが登録されている場合、CLI は LaunchAgent 設定を変更してはならない（MUST NOT overwrite）。ジョブは共有 `jobs.json` に追加され、Desktop スケジューラによって実行される。
3. **安全なアンインストール保護**:
   - `uninstall-scheduler` は現在の所有者を確認し、Desktop 所有である場合はエラーを返してアンインストールを拒絶しなければならない（MUST NOT uninstall Desktop-owned scheduler）。CLI 所有である場合のみアンロードおよび plist 削除を行う。

### CLI-CMD-005: 共有 JobStore と並行性制御

1. **同一データストアの共有**:
   - CLI は常に Desktop GUI と同一の `~/.codex-scheduler/jobs.json` を読み書きしなければならない（MUST）。
2. **相互可視性（Mutual Observability）**:
   - CLI で `schedule` したジョブは直ちに Desktop GUI の一覧およびメトリクスに反映されなければならない（MUST）。
   - Desktop GUI で作成されたジョブは CLI の `list` / `show` で閲覧・キャンセル可能でなければならない（MUST）。
3. **排他制御（Concurrency Safety）**:
   - GUI、CLI、およびバックグラウンドスケジューラが同時にアクセスした場合でも、ファイルロック（`fs2`）およびアトミッククレーム（`claim_due_jobs`）により、データの破損やジョブの二重実行を発生させてはならない（MUST NOT）。

## 検証ルール

- `codex-scheduler --help` および `codex-scheduler --version` が正しく表示されること。
- `schedule --json`, `list --json`, `show --json`, `cancel --json`, `delete --json`, `status --json` の標準出力が JSON パース可能であること。
- CLI-only 環境で `schedule` 実行時に LaunchAgent が登録され、`owner` が `cli` となること。
- Desktop 所有の LaunchAgent が存在する場合、CLI の `schedule` が Desktop 登録を上書きせず維持すること。
- Desktop 所有時に `uninstall-scheduler` がエラーで終了し、LaunchAgent が保護されること。

## 互換性

- 従来の引数形式（`--session-id`, `--cwd`, `--prompt`, `--at`）および日時指定ルール（OS-SCHED-005）と完全互換。
- 既存の `jobs.json` ファイル形式をそのまま利用可能。

## 非目標

- CLI 自身による常駐 HTTP / Webhook サーバーの起動。
- 対話型 TUI（Terminal UI）ダッシュボードの実装。

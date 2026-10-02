# 用語集（Terminology）

Status: Approved

| 用語 | 英語 / 識別子 | 定義 |
| :--- | :--- | :--- |
| **ジョブ** | `Job` | スケジュールされた1回のプロンプト送信タスク。対象Provider、セッションID、作業ディレクトリ、プロンプト、実行予定時刻、リトライ設定、実行ステータスを持つ。 |
| **セッションID** | `Session ID` | AIサービス（Codex等）側で会話スレッドを一意に特定するID。 |
| **プロンプト** | `Prompt` | リセット時にセッションへ送信する指示文。デフォルトは `"continue"`。 |
| **作業ディレクトリ** | `Working Directory (cwd)` | AI CLIコマンドを実行するローカルプロジェクトのルートパス。 |
| **プロバイダ** | `Provider` | AIコーディングサービスの実行エンジン（`Codex` 等）。 |
| **プロバイダアダプタ** | `Provider Adapter` | 各AI CLI（`codex` コマンド等）の引数組み立て、実行、エラー判定（Quota超過か否か等）を抽象化するインターフェース。 |
| **リトライポリシー** | `Retry Policy` | 実行失敗（特に利用枠上限エラー）時に、一定間隔（例: 5分）で再試行するルール。 |
| **OSスケジューラ** | `OS Scheduler` | macOSの `launchd`（LaunchAgent）または Windowsの `Task Scheduler`。アプリ停止中も指定時刻にworkerを起動する。 |
| **ワーカー** | `Worker` | 指定時刻にOSスケジューラから起動され、ジョブの実行・リトライ・ログ記録を行う独立したバックグラウンドCLIプロセス。 |
| **ジョブステータス** | `Job Status` | `scheduled`（待機中）, `running`（実行中）, `retrying`（リトライ待機中）, `succeeded`（成功）, `failed`（失敗）, `cancelled`（取消済）。 |
| **Desktop-first** | `Desktop-first` | 普段はデスクトップGUIアプリ上で作業し、夜間だけ外部からセッションをresumeし、翌朝再びデスクトップGUIで継続する運用形態。 |

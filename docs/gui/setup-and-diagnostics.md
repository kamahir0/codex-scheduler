# GUI仕様: 環境診断・権限セットアップ（Environment Setup & Diagnostics）

Status: Approved

## 目的

初回インストール時や利用開始時に、ユーザーの環境で Codex CLI が利用可能か、必要なOS権限が揃っているかを自動診断し、問題がある場合は Ant Design の視覚的アラートとガイダンスで解決を支援する仕様を定める。

## 規範要件

### DIAG-UI-001: 起動時ヘルスチェック

アプリケーション起動時に、以下の項目を非同期で確認しなければならない（MUST）。

1. **Codex CLI の存在確認**:
   - `codex` コマンドが PATH または標準ディレクトリから解決可能か。
2. **OSスケジューラ権限確認**:
   - macOS: `~/Library/LaunchAgents/` へのアクセスおよび書き込みが可能か。
   - Windows: Task Scheduler コマンドへのアクセスが可能か。

### DIAG-UI-002: 未検出時のアラートバナー表示

Codex CLI が検出されなかった場合、メイン画面上部に Ant Design の `Alert`（`type="warning"`）を表示しなければならない（MUST）。

- **メッセージ**: 「OpenAI Codex CLI が検出されませんでした」
- **説明**: 「深夜の自動再開を行うには、システムに `codex` コマンドがインストールされ、PATHが通っている必要があります。」
- **アクション**: 「セットアップ手順」ボタン（クリックでモーダルまたは手順書を開く）。

### DIAG-UI-003: システム情報・権限診断モーダル

ヘッダーの設定/情報アイコンから開ける環境診断モーダルを提供し、以下を表示しなければならない（SHOULD）。

- OS / アーキテクチャ
- 検出された `codex` コマンドパス（見つからない場合は `未検出`）
- スケジューラ実行バイナリパス（macOS: アプリ本体実行ファイルパス、Windows: Worker CLIパス）
- スケジューラ実行パス一致状態（現在アプリ実行パスとLaunchAgent登録パスの一致）
- ジョブデータ保存先（`~/.codex-scheduler/jobs.json`）
- LaunchAgent / Task Scheduler 登録状況（登録済 / 未登録 / 登録エラー）

# 仕様変更: 実行中ジョブのリアルタイムログ表示（Active Execution Log Streaming & Live View）

Status: Applied

## Affected Specifications

- `docs/gui/job-scheduling.md`: 3. 実行ログ詳細ドロワー（Execution Logs Drawer）
- `apps/gui/src-tauri/src/lib.rs`: Tauri コマンド `get_active_job_log`
- `apps/gui/src/api.ts`: APIクライアント `getActiveJobLog`
- `apps/gui/src/types.ts`: `ActiveLogResponse` 型定義
- `apps/gui/src/components/LogsDrawer.tsx`: リアルタイムログストリーミング表示・自動ポーリング・自動スクロール

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - 「GUI にリアルタイムログ表示機能を追加して」（長時間ジョブ実行中の思考・進捗ログが見えない不安を解消する）。
- **Agent Decision**:
  - **バックエンド IPC（Tauri Command）**:
    - `get_active_job_log(id: String) -> Result<ActiveLogResponse, String>` を追加。
    - コアクレートの `runner::runner_log_path`、`runner::is_runner_active`、`runner::check_codex_process_liveness` を利用し、現在の試行番号（`execution_history.len() + 1`）、OS プロセス生存状態、およびログファイルの最新内容（最大 64KB bounded tail）を安全に読み取って返却する。
    - ログファイルが存在しない、または未作成の初期段階では空文字列を返し、エラーで落とさない（Graceful Degrade）。
  - **フロントエンド UI/UX（LogsDrawer & Auto-polling）**:
    - 対象ジョブが `Running` 状態かつドロワーが開いている間、2秒間隔で `getActiveJobLog` を自動ポーリングする。
    - ドロワー上部に「実行中ライブログ（Live Execution Log）」カードを配置。
    - ヘッダーにスピナー付きの「リアルタイム追尾中」バッジ、最終更新時刻、および「ログをコピー」ボタンを表示。
    - ログ表示ブロックはターミナル風（ダーク背景・等幅フォント、最大高さ 360px、縦スクロール）とし、ログ追記時に最下部へ自動スクロールする。
    - ユーザーがスクロール位置を上に移動させた場合でも操作を阻害しないよう配慮する。
    - ジョブが `Running` から完了ステータス（`Succeeded` / `Failed` / `Retrying` 等）へ遷移した場合は、ポーリングを即座に停止し、通常の試行履歴表示へ更新する。

## 提案する差分（Proposed Delta）

- `docs/gui/job-scheduling.md`:
  - 実行ログ詳細ドロワーの仕様に「実行中（Running）ジョブのリアルタイムログ表示（Live Stream Log）」を明記。
- `apps/gui/src-tauri/src/lib.rs`:
  - `ActiveLogResponse` 構造体と `get_active_job_log` コマンドを追加し、`generate_handler!` に登録。
- `apps/gui/src/types.ts`:
  - `ActiveLogResponse` インターフェースを追加。
- `apps/gui/src/api.ts`:
  - `getActiveJobLog(id: string)` 関数を追加。
- `apps/gui/src/components/LogsDrawer.tsx`:
  - `Running` 時のアクティブログ表示UI、定期ポーリングタイマー、自動スクロール処理を実装。

## 互換性（Compatibility）

- 既存の `Job` スキーマ、`jobs.json`、CLI、OS スケジューラへの影響なし。
- 既存のストリーミングログ保持機構（`CODEX-RESUME-009`）を read-only で参照するため、実行中プロセスの排他性やファイル整合性を損なわない。

## レビュー（Review）

- **Blocking Issues**: None identified.
- **Non-blocking Issues**: None identified.
- **Approved as Proposed**: Yes.
- **Autonomous approval eligibility**:
  - Eligible: Yes
  - Human gate: None (ユーザーの明示的なUI機能追加指示に基づくフロントエンドおよび読み取りIPCの拡張)
  - Rationale: データモデルやスケジューラコアへの破壊的変更を含まず、read-only なログ可観測性の拡張であるため。

## 承認記録（Approval Record）

- **Approval mode**: Autonomous (Human-directed)
- **Basis**: ユーザーの明示指示（「GUI にリアルタイムログ表示機能を追加して」）
- **Status Transition**: `Proposed` -> `Approved`

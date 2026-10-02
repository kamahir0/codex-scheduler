# Current Objective

## Objective

**Initial Product Foundation & End-to-End Implementation of codex-scheduler。仕様駆動開発（Specification-Driven Development）に基づき、canonical spec、Rust core、CLI worker、OSスケジューラ連携、およびAnt DesignによるモダンなTauri 2デスクトップアプリを完成させる。**

## Completion slices

- **Slice 1: 仕様群の策定（Canonical Specifications）**:
  - `docs/specs/job-lifecycle.md`: ジョブのデータ構造、状態遷移、永続化。
  - `docs/specs/codex-adapter.md`: Codex CLIの非対話resume実行、Quotaエラー検知、出力パース。
  - `docs/specs/retry-policy.md`: Quotaリセット待ちリトライアルゴリズム。
  - `docs/specs/os-scheduler.md`: macOS launchd / Windows Task Scheduler の連携仕様。
  - `docs/gui/app-shell.md`: Ant Designを用いたモダンデスクトップUI仕様、テーマ、通知。
  - `docs/gui/job-scheduling.md`: ジョブ作成・一覧・ログ詳細・操作インタラクション仕様。
- **Slice 2: Rust Core & CLI基盤（crates/）**:
  - `codex-scheduler-core`: ジョブモデル、永続化ストア、CodexAdapter、RetryEngine、OSスケジューラ登録。
  - `codex-scheduler-cli`: バックグラウンドworker実行コマンド（`run-job`）とCLI管理機能。
- **Slice 3: Tauri 2 デスクトップアプリ & Ant Design フロントエンド（apps/gui/）**:
  - Tauri 2 RustバックエンドとIPCハンドラ実装。
  - Ant Design (`antd`) を全面活用したモダンUI（Dashboard, Job Creator, Job List with Status Badges, Execution Logs Drawer, Dark/Light Theme）。
- **Slice 4: 自動テスト & 検証（Verification）**:
  - Coreクレートのユニットテスト、CLI結合テスト、フロントエンドTypeScript型チェック及びビルド検証。

## Authority / output routing

- Product vision: [`docs/product/vision.md`](product/vision.md)
- Terminology: [`docs/product/terminology.md`](product/terminology.md)
- Specifications: [`docs/specs/`](specs/)
- GUI specifications: [`docs/gui/`](gui/)
- Architecture decisions: [`docs/adr/`](adr/)

## Explicit non-scope

- 外部クラウド認証・リモートサーバー同期。
- Codex以外の未確定AIツール（Cursor等の非公式ハック）の実装（アダプタのインターフェース設計のみ担保）。
- 完全自作の独自CSSフレームワーク（Ant Designの機能をフル活用する）。

# 仕様変更: 初期基盤とCodexスケジューラ仕様（Specification change: Initial Foundation & Codex Scheduler）

Status: Applied

## Affected Specifications

- `docs/specs/job-lifecycle.md`: SCHED-JOB-001 〜 005
- `docs/specs/codex-adapter.md`: CODEX-RESUME-001 〜 006
- `docs/specs/retry-policy.md`: RETRY-POLICY-001 〜 004
- `docs/specs/os-scheduler.md`: OS-SCHED-001 〜 004
- `docs/gui/app-shell.md`: APP-SHELL
- `docs/gui/job-scheduling.md`: JOB-SCHED-GUI

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - `masterdata` プロジェクトと同様の仕様駆動開発を実施すること。
  - フロントエンドUIデザインは Ant Design を使用し、フレームワークの機能を活用してモダンにすること。
  - 共有されたChatGPT会話（Codex利用可否調査）に基づき、デスクトップ版Codex利用者のための夜間トークン復活スケジューラを構築すること。
- **Agent Decision**:
  - バックエンド・コアとして Rust (`codex-scheduler-core` / `codex-scheduler-cli`) を構築し、デスクトップフレームワークとして Tauri 2 を採用。
  - macOS launchd および Windows Task Scheduler へのジョブ登録生成を標準装備。
  - Quota枯渇エラーを標準出力/エラー出力からパターン検出し、最大試行回数まで自動再試行するリトライエンジンを内蔵。

## 提案する差分（Proposed Delta）

- 初期仕様セットの作成（`job-lifecycle`, `codex-adapter`, `retry-policy`, `os-scheduler`, `app-shell`, `job-scheduling`）。
- canonical status: `Approved`

## 互換性（Compatibility）

- 新規プロジェクトのため後方互換性の破壊はなし。

## 受け入れと実装への影響（Acceptance and Implementation Impact）

- Rust Core クレート、CLI クレート、Tauri 2 アプリケーション（React + Ant Design）の実装。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (Human Objectiveの範囲内、Human gate なし)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Agent-autonomous
- **Basis**: ユーザーの明示的リクエスト「masterdata と同様の仕様駆動開発」「Ant Design によるモダンUI」および事前合意された会話内容。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

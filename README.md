# Codex Scheduler

利用枠（Quota / Rate Limit）リセット時に自動でセッションを再開する、ローカルファーストのスケジュール管理デスクトップアプリケーション。

普段Codexデスクトップアプリ（GUI）で作業している開発者が、就寝前にセッションIDと作業ディレクトリを登録しておくことで、夜間のトークン制限解除時刻（深夜2時など）に外部から自動で `"continue"` プロンプトを送信。翌朝デスクトップアプリを開いた際に、**何事もなかったかのように同一セッションの続きから作業を再開**できます。

---

## 主な特徴

- **Desktop-first 運用に完全準拠**:
  Codexデスクトップアプリ、CLI、IDEで同一のローカルセッションDBが共有される仕組みを活用し、深夜に外部から非対話で再開（`codex exec resume <session_id> "continue"`）を実行。
- **OSネイティブスケジューラ連携**:
  アプリ自身が深夜まで常駐するのではなく、OSのネイティブスケジューラ（macOS: `launchd`, Windows: `Task Scheduler`）に委譲。PCが待機状態でも確実にworkerが起動。
- **インテリジェント・リトライポリシー**:
  リセット予定時刻の微小なズレ（数分の遅延）や一時的な利用制限超過（HTTP 429等）を標準出力/エラー出力から自動判定し、設定間隔（例: 5分おき）で最大試行回数まで自動再試行。
- **Ant Design によるモダンなUI**:
  ダークモード／ライトモード切り替え、カウントダウン表示、ステータスバッジ、直感的なジョブ登録モーダル、ターミナル風の実行履歴ログドロワー。

---

## ダウンロード & インストール

[GitHub Releases](https://github.com/kamahir0/codex-scheduler/releases) より最新インストーラを取得できます。

- **macOS (Apple Silicon / M1〜M4)**: `Codex-Scheduler-<version>-macos-arm64.dmg`
- **Windows (64-bit)**: `Codex-Scheduler-<version>-windows-x64.exe`

インストール手順や権限付与の詳細は [インストール & 権限セットアップガイド](docs/setup-guide.md) をご覧ください。

---

## 仕様駆動開発（Specification-Driven Development）

本プロジェクトは [`masterdata`](https://github.com/kamahir0/masterdata) と同様の厳格な仕様駆動開発（Specification-Driven Development）を採用しています。

- **操作カーネル**: [`AGENTS.md`](AGENTS.md)
- **開発ワークフロー**: [`docs/execution-workflow.md`](docs/execution-workflow.md)
- **現在の開発目標**: [`docs/current-objective.md`](docs/current-objective.md)
- **開発状態チェックポイント**: [`docs/execution-state.md`](docs/execution-state.md)
- **プロダクトビジョン**: [`docs/product/vision.md`](docs/product/vision.md)
- **用語集**: [`docs/product/terminology.md`](docs/product/terminology.md)
- **承認済み仕様（Canonical Specs）**:
  - [ジョブライフサイクルと永続化 (`SCHED-JOB`)](docs/specs/job-lifecycle.md)
  - [Codex プロバイダアダプタ (`CODEX-RESUME`)](docs/specs/codex-adapter.md)
  - [リトライポリシー (`RETRY-POLICY`)](docs/specs/retry-policy.md)
  - [OSスケジューラ連携 (`OS-SCHED`)](docs/specs/os-scheduler.md)
  - [デスクトップ配布・パッケージング (`DELIVERY-BUNDLE`)](docs/specs/desktop-delivery.md)
- **GUI仕様**:
  - [アプリケーションシェル](docs/gui/app-shell.md)
  - [ジョブ管理テーブル・モーダル・ログ](docs/gui/job-scheduling.md)
  - [環境診断・権限セットアップ](docs/gui/setup-and-diagnostics.md)
- **ユーザー向けガイド**:
  - [インストール & 権限セットアップガイド](docs/setup-guide.md)
- **設計決定記録（ADR）**:
  - [ADR 0001: Tauri 2 と OSスケジューラによるヘッドレス実行](docs/adr/0001-tauri-and-os-scheduler-architecture.md)
  - [ADR 0002: Ant Design によるモダンUIシステム](docs/adr/0002-ant-design-frontend-ui.md)

---

## ディレクトリ構成

```text
codex-scheduler/
├── AGENTS.md                          # AI / 開発者 operating kernel
├── Cargo.toml                         # Cargo workspace
├── package.json                       # npm workspace
├── crates/
│   ├── codex-scheduler-core/          # ジョブ管理、Codexアダプタ、リトライ、OSスケジューラ
│   └── codex-scheduler-cli/           # バックグラウンドworker & CLIツール
├── apps/
│   └── gui/                           # Tauri 2 デスクトップアプリ
│       ├── src/                       # React 19 + TypeScript + Ant Design UI
│       └── src-tauri/                 # Tauri 2 Rust バックエンド
├── docs/                              # 仕様・設計・ガバナンスドキュメント
└── skills/                            # 仕様駆動開発用スキルセット
```

---

## 開発と実行

### 必要要件
- Rust 1.85+ (`cargo`)
- Node.js 20+ (`npm`)
- OpenAI Codex CLI (`codex` コマンドがインストールされていること)

### テストの実行
```bash
# Rust コア & CLI テスト
cargo test -p codex-scheduler-core -p codex-scheduler-cli

# フロントエンド型チェック & ビルド
npm --workspace @codex-scheduler/gui run lint
npm --workspace @codex-scheduler/gui run build
```

### デスクトップアプリの起動
```bash
npm run gui:dev
```

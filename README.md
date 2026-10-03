# Codex Scheduler

利用枠（Quota / Rate Limit）リセット時に自動でセッションを再開する、ローカルファーストのスケジュール管理デスクトップアプリケーション。

<p align="left">
  <img src="https://img.shields.io/badge/Rust-000000?style=flat-square&logo=rust&logoColor=white" alt="Rust" />
  <img src="https://img.shields.io/badge/Tauri_v2-24C8D8?style=flat-square&logo=tauri&logoColor=white" alt="Tauri v2" />
  <img src="https://img.shields.io/badge/React_19-20232A?style=flat-square&logo=react&logoColor=61DAFB" alt="React 19" />
  <img src="https://img.shields.io/badge/TypeScript-3178C6?style=flat-square&logo=typescript&logoColor=white" alt="TypeScript" />
  <img src="https://img.shields.io/badge/Ant_Design-0170FE?style=flat-square&logo=antdesign&logoColor=white" alt="Ant Design" />
  <img src="https://img.shields.io/badge/Vite-646CFF?style=flat-square&logo=vite&logoColor=white" alt="Vite" />
  <img src="https://img.shields.io/badge/macOS-000000?style=flat-square&logo=apple&logoColor=white" alt="macOS" />
  <img src="https://img.shields.io/badge/Windows-0078D6?style=flat-square&logo=windows&logoColor=white" alt="Windows" />
</p>

![Codex Scheduler メイン画面](docs/images/screenshot.png)

普段CodexデスクトップアプリやCLIで開発している際、トークン利用制限（Rate Limit）に達して作業が中断してしまうことがあります。  
**Codex Scheduler** を使えば、就寝前にセッションIDと作業ディレクトリを登録しておくだけで、夜間のリセット時刻（例: 深夜2時）に自動で `"continue"` を送信し、バックグラウンドで作業を再開。  
翌朝PCを開いた際には、**何事もなかったかのように同一セッションの続きから作業を完了・継続**できます。

---

## 主な特徴

- 🔄 **Desktop-first 運用に完全準拠**  
  Codexデスクトップアプリ、CLI、IDEで同一のローカルセッションDBが共有される仕組みを活用し、深夜に外部から非対話で安全に再開（`codex exec resume <session_id> "continue"`）を実行します。
- ⏰ **OSネイティブスケジューラ連携（省電力 & 高信頼性）**  
  アプリ自身が深夜まで常駐し続ける必要はありません。macOS では `launchd`（LaunchAgent）、Windows では Task Scheduler の OS 標準スケジューラに委譲するため、アプリを終了していても指定時刻以降にバックグラウンド起動して確実に実行します（定期確認により通常1分以内に開始）。
- 🛡️ **インテリジェント・リトライポリシー**  
  リセット予定時刻の微小なズレ（数分の遅延）や一時的な利用制限超過（HTTP 429等）を自動判定。設定間隔（例: 5分おき）で成功するまで自動再試行します。
- 🎨 **Ant Design による洗練されたモダンUI**  
  ダークモード／ライトモード切り替え、カウントダウンタイマー、直感的なジョブ登録モーダル、ターミナル風の実行履歴ログ表示を標準装備。

---

## 技術スタック

| 分野 | 技術 |
| :--- | :--- |
| **バックエンド / コア** | [Rust](https://www.rust-lang.org/) / [Tauri v2](https://tauri.app/) |
| **フロントエンド** | [React 19](https://react.dev/) / [TypeScript](https://www.typescriptlang.org/) / [Vite](https://vitejs.dev/) |
| **UIコンポーネント** | [Ant Design 5](https://ant.design/) / [Lucide Icons](https://lucide.dev/) |
| **OSスケジューラ** | macOS `launchd` (LaunchAgent) / Windows Task Scheduler |

---

## インストール & クイックスタート

[GitHub Releases](https://github.com/kamahir0/codex-scheduler/releases) より、お使いの環境に合わせたパッケージをダウンロードしてください。

### 配布パッケージ一覧

| 種別 | OS / アーキテクチャ | 配布ファイル | 用途 |
| :--- | :--- | :--- | :--- |
| **Desktop GUI** | macOS (Apple Silicon) | `Codex-Scheduler-<version>-macos-arm64.dmg` | GUIデスクトップアプリ |
| **Desktop GUI** | Windows (64-bit) | `Codex-Scheduler-<version>-windows-x64.exe` | GUIデスクトップインストーラ |
| **Standalone CLI** | macOS (Apple Silicon) | `Codex-Scheduler-CLI-<version>-macos-arm64` | 単体CLIバイナリ (`codex-scheduler`) |
| **Standalone CLI** | Windows (64-bit) | `Codex-Scheduler-CLI-<version>-windows-x64.exe` | 単体CLIバイナリ (`codex-scheduler.exe`) |

### 前提条件
本スケジューラはバックグラウンドで `codex` コマンドを実行します。システムに OpenAI Codex CLI がインストールされていることを確認してください。
```bash
codex --version
```

### CLI 利用例 (macOS / Windows 共通)
```bash
# ジョブの登録（2時間後に再開）
codex-scheduler schedule --session-id "sess-123" --cwd "/path/to/project" --at "+120"

# 登録ジョブ一覧の表示
codex-scheduler list --json

# スケジューラ稼働状態・所有権の確認
codex-scheduler status
```

> [!TIP]
> 初回起動時のOS警告対処（macOS Gatekeeper / Windows SmartScreen）、PATH設定、スケジューラ常設登録の仕組み、およびトラブルシューティングの詳細は **[セットアップガイド](docs/setup-guide.md)** をご覧ください。

---

## 開発・ビルド（コントリビューター向け）

```bash
# 依存関係のインストール
npm install

# 開発用デスクトップアプリの起動
npm run gui:dev

# テスト実行 & ビルド確認
cargo test -p codex-scheduler-core -p codex-scheduler-cli
npm --workspace @codex-scheduler/gui run lint
npm --workspace @codex-scheduler/gui run build
```

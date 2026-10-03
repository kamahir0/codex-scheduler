# Codex Scheduler

利用枠（Quota / Rate Limit）リセット時に自動でセッションを再開する、ローカルファーストのスケジュール管理デスクトップアプリケーション。

![Codex Scheduler メイン画面](docs/images/screenshot.png)

普段CodexデスクトップアプリやCLIで開発している際、トークン利用制限（Rate Limit）に達して作業が中断してしまうことがあります。  
**Codex Scheduler** を使えば、就寝前にセッションIDと作業ディレクトリを登録しておくだけで、夜間のリセット時刻（例: 深夜2時）に自動で `"continue"` を送信し、バックグラウンドで作業を再開。  
翌朝PCを開いた際には、**何事もなかったかのように同一セッションの続きから作業を完了・継続**できます。

---

## 主な特徴

- 🔄 **Desktop-first 運用に完全準拠**  
  Codexデスクトップアプリ、CLI、IDEで同一のローカルセッションDBが共有される仕組みを活用し、深夜に外部から非対話で安全に再開（`codex exec resume <session_id> "continue"`）を実行します。
- ⏰ **OSネイティブスケジューラ連携（省電力 & 高信頼性）**  
  アプリ自身が深夜まで常駐し続ける必要はありません。OS標準のスケジューラ（macOS: `launchd`, Windows: `Task Scheduler`）に委譲するため、アプリを終了していても指定時刻にバックグラウンド起動して確実に実行します。
- 🛡️ **インテリジェント・リトライポリシー**  
  リセット予定時刻の微小なズレ（数分の遅延）や一時的な利用制限超過（HTTP 429等）を自動判定。設定間隔（例: 5分おき）で成功するまで自動再試行します。
- 🎨 **Ant Design による洗練されたモダンUI**  
  ダークモード／ライトモード切り替え、カウントダウンタイマー、直感的なジョブ登録モーダル、ターミナル風の実行履歴ログ表示を標準装備。

---

## インストール手順

[GitHub Releases](https://github.com/kamahir0/codex-scheduler/releases) より、お使いのOSに合わせたインストーラをダウンロードしてください。

| OS | アーキテクチャ | インストーラ |
| :--- | :--- | :--- |
| **macOS (Apple Silicon)** | M1 / M2 / M3 / M4 | `Codex-Scheduler-<version>-macos-arm64.dmg` |
| **Windows** | 64-bit | `Codex-Scheduler-<version>-windows-x64.exe` |

### 前提条件
本スケジューラはバックグラウンドで `codex` コマンドを実行します。端末に OpenAI Codex CLI がインストールされていることを確認してください。
```bash
codex --version
```

### macOS の場合
1. ダウンロードした `.dmg` ファイルを開き、`Codex Scheduler.app` を **Applications（アプリケーション）** フォルダへドラッグ＆ドロップします。
2. アプリケーションフォルダから起動します。
> [!NOTE]
> 初回起動時に「開発元を確認できないため開けません」と表示された場合は、Finder でアプリを **右クリック（Control + クリック）して「開く」** を選択するか、ターミナルで以下を実行して隔離属性を解除してください：
> ```bash
> xattr -d com.apple.quarantine /Applications/Codex\ Scheduler.app
> ```

### Windows の場合
1. ダウンロードした `.exe` ファイルを実行し、ウィザードに従ってインストールします。
> [!NOTE]
> Microsoft Defender SmartScreen が表示された場合は、「詳細情報」をクリックしてから「実行」を選択してください。

より詳しい権限仕様やバックグラウンドWorkerの仕組みについては [インストール & 権限セットアップガイド](docs/setup-guide.md) をご覧ください。

---

## かんたんな使い方

### 1. セッションIDを確認
CodexデスクトップアプリやCLIで作業中、レート制限に達したら現在のセッションID（UUID）をコピーします。  
（デスクトップアプリの画面上部や、CLIのセッション一覧から取得できます）

### 2. ジョブを登録
Codex Scheduler を開き、右上の **「+ 新規ジョブ登録」** をクリックします。
- **セッションID**: コピーしたセッションIDを入力
- **作業ディレクトリ**: プロジェクトのルートディレクトリを選択
- **実行予定日時**: トークンリセット時刻（例: 深夜 02:00）を指定
- **リトライ設定**: （任意）制限解除の遅延に備えてリトライ間隔と回数を指定

### 3. あとは寝て待つだけ
登録が完了したら、Codex Scheduler アプリを閉じて構いません。  
指定時刻になると、OSスケジューラがバックグラウンドで Worker を呼び出し、セッションを再開します。

### 4. 翌朝、作業の続きを確認
翌朝デスクトップアプリやターミナルを開けば、夜間に実行されたタスクの成果がそのまま同一セッションに反映されています。  
Codex Scheduler のログアイコンをクリックすれば、夜間の標準出力やエラーログも確認できます。

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

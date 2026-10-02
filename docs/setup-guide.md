# インストール & 権限セットアップガイド

GitHub Releases からダウンロードして利用を開始するまでの手順書です。

---

## 1. ダウンロード

[GitHub Releases](https://github.com/...) の最新リリース（Latest Release）ページから、ご使用のOSに合ったインストーラをダウンロードします。

| OS | アーキテクチャ | ダウンロードファイル |
| :--- | :--- | :--- |
| **macOS (Apple Silicon)** | M1 / M2 / M3 / M4 | `Codex-Scheduler_<version>_aarch64.dmg` |
| **macOS (Intel)** | Intel Core | `Codex-Scheduler_<version>_x64.dmg` |
| **Windows** | 64-bit | `Codex-Scheduler_<version>_x64-setup.exe` (または `.msi`) |

---

## 2. インストールと初回起動手順

### macOS の場合

1. ダウンロードした `.dmg` ファイルをダブルクリックしてマウントします。
2. 表示されたウィンドウで、`Codex Scheduler.app` を **Applications（アプリケーション）** フォルダへドラッグ＆ドロップします。
3. Applications フォルダから `Codex Scheduler.app` を起動します。

#### 【重要】「開発元を確認できないため開けません」と表示された場合
公的Apple証明書での公証（Notarization）前のオープンソースビルドの場合、macOS Gatekeeper により起動がブロックされる場合があります。以下のいずれかで安全に起動できます。

* **方法 A（推奨・GUI操作）**:
  1. Finderの「アプリケーション」フォルダで `Codex Scheduler.app` を **右クリック（Controlキーを押しながらクリック）** します。
  2. メニューから **「開く」** を選択します。
  3. 警告ダイアログに「開く」ボタンが表示されるので、それをクリックします（次回以降は通常起動できます）。

* **方法 B（ターミナル操作）**:
  ターミナルで以下のコマンドを実行して隔離属性（quarantine）を解除します：
  ```bash
  xattr -d com.apple.quarantine /Applications/Codex\ Scheduler.app
  ```

#### 権限について
- **スケジューラ登録**: ユーザー単位の LaunchAgent（`~/Library/LaunchAgents/`）を使用するため、管理者権限（sudo）は不要です。
- **プロジェクトフォルダへのアクセス**: 初回に作業ディレクトリを参照する際、macOSから「フォルダへのアクセスを求めています」とダイアログが出た場合は「許可」を選択してください。

---

### Windows の場合

1. ダウンロードした `.exe` または `.msi` ファイルをダブルクリックして実行します。
2. 画面の指示に従ってインストールを完了します。

#### 【重要】「Windows によって PC が保護されました」と表示された場合
Microsoft Defender SmartScreen が表示された場合：
1. ダイアログ内の **「詳細情報」** をクリックします。
2. 表示された **「実行」** ボタンをクリックします。

---

## 3. 前提条件の確認（Codex CLI）

本スケジューラは、深夜にバックグラウンドで `codex exec resume <session_id> "continue"` を実行します。
そのため、お使いの端末で **OpenAI Codex CLI** が利用可能である必要があります。

ターミナルまたはPowerShellで以下を実行し、正常に応答することを確認してください：
```bash
codex --version
```

もし「command not found」となる場合は、Codex のセットアップを確認し、シェル環境変数（PATH）に `codex` の実行パスが含まれていることを確認してください。
（一般的な配置場所: `~/.local/bin`, `/opt/homebrew/bin`, `/usr/local/bin`, `~/.npm-global/bin` 等）

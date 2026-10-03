# インストール & 権限セットアップガイド

GitHub Releases からダウンロードして利用を開始するまでの手順書です。

---

## 1. ダウンロード

[GitHub Releases](https://github.com/kamahir0/codex-scheduler/releases) の最新リリース（Latest Release）ページから、ご使用のOSに合ったインストーラをダウンロードします。

### 配布パッケージ一覧（4種類）

| 種別 | OS / アーキテクチャ | ダウンロードファイル | 説明 |
| :--- | :--- | :--- | :--- |
| **Desktop GUI** | macOS (Apple Silicon M1〜M4) | `Codex-Scheduler-<version>-macos-arm64.dmg` | GUIデスクトップアプリ（単体で予約実行完結） |
| **Desktop GUI** | Windows (64-bit) | `Codex-Scheduler-<version>-windows-x64.exe` | GUIデスクトップインストーラ（単体で予約実行完結） |
| **Standalone CLI** | macOS (Apple Silicon M1〜M4) | `Codex-Scheduler-CLI-<version>-macos-arm64` | 単体CLI実行ファイル（GUI不要、単体で予約実行完結） |
| **Standalone CLI** | Windows (64-bit) | `Codex-Scheduler-CLI-<version>-windows-x64.exe` | 単体CLI実行ファイル（GUI不要、単体で予約実行完結） |

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

#### 権限について & 完全無料・シングル実行ファイルアーキテクチャ
- **完全無料配布方針（Apple Developer登録不要）**:
  - 本プロジェクトは、すべての開発者が完全無料で利用・配布できるよう、Apple Developer Program の有償アカウントを必須とせず、ad-hoc 署名（`signingIdentity: "-"`）でビルドされています。
- **1 App / 1 Executable / 2 Modes による Gatekeeper 根本対策**:
  - バックグラウンドスケジューラ（LaunchAgent）は、外部の別バイナリではなく、`Codex Scheduler.app` 内のメイン実行ファイル自身をヘッドレスモード（`--scheduler-tick`）で起動します。
  - そのため、**ユーザーが初回起動時に `Codex Scheduler.app` を許可（Control+クリックで「開く」）するだけで、バックグラウンド実行もすべて同一の承認済みバイナリとして動作します**。外部の `codex-scheduler-cli` に対する追加の Gatekeeper 警告やブロックが発生することはありません。
- **バックグラウンド通知の最小化（1 App = 1 LaunchAgent）**:
  - macOS 13 (Ventura) 以降、新しい LaunchAgent が登録されるたびに「バックグラウンド項目が追加されました」と通知されます。
  - Codex Scheduler では、ジョブを1件追加するたびに LaunchAgent を増設するのではなく、**アプリ専用の単一常設 LaunchAgent（`dev.codexscheduler.scheduler.plist`）が内部ですべてのジョブ（待機・リトライ・完了）を管理・定期実行（1分間隔）** します（※ジョブは指定時刻以降、次回の定期チェック時に実行されるため、通常0〜約60秒の遅延があります）。
  - 初回起動時（またはアプリ移動・更新時）に1回だけパスが登録・確認され、以後のジョブ追加・編集・削除で通知が出ることはありません。
- **スケジューラ登録権限**: ユーザー単位の LaunchAgent（`~/Library/LaunchAgents/`）を使用するため、管理者権限（sudo）は不要です。
- **プロジェクトフォルダへのアクセス**: 初回に作業ディレクトリを参照する際、macOSから「フォルダへのアクセスを求めています」とダイアログが出た場合は「許可」を選択してください。

---

### Windows の場合

1. ダウンロードした `.exe` ファイルをダブルクリックして実行します。
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

---

## 4. Standalone CLI (`codex-scheduler`) のセットアップ & 利用

GUI不要の環境やCI/CD、スクリプト自動化向けに、単一バイナリの CLI ディストリビューションを提供しています。

### インストール手順 (macOS)
1. Releases から `Codex-Scheduler-CLI-<version>-macos-arm64` をダウンロードします。
2. 実行権限を付与し、PATHの通った場所（例: `/usr/local/bin` や `~/.local/bin`）へ配置します：
   ```bash
   chmod +x Codex-Scheduler-CLI-*-macos-arm64
   mv Codex-Scheduler-CLI-*-macos-arm64 ~/.local/bin/codex-scheduler
   ```
3. バージョンを確認します：
   ```bash
   codex-scheduler --version
   ```

### 主なコマンド
- **ジョブの新規予約**:
  ```bash
  # 2時間後に再開
  codex-scheduler schedule --session-id "sess-123" --cwd "/path/to/project" --at "+120"

  # ISO 8601 時刻指定
  codex-scheduler schedule --session-id "sess-123" --cwd "/path/to/project" --at "2026-10-04T02:00:00Z"
  ```
- **ジョブ一覧表示**:
  ```bash
  codex-scheduler list
  codex-scheduler list --json
  ```
- **ジョブ詳細・履歴確認**:
  ```bash
  codex-scheduler show <job-id>
  ```
- **ジョブのキャンセル / 削除**:
  ```bash
  codex-scheduler cancel <job-id>
  codex-scheduler delete <job-id>
  ```
- **システム & スケジューラステータス確認**:
  ```bash
  codex-scheduler status
  codex-scheduler status --json
  ```
- **OSスケジューラ常設サービスの登録 / アンインストール**:
  ```bash
  codex-scheduler install-scheduler
  codex-scheduler uninstall-scheduler
  ```

---

## 5. Desktop と CLI の共存運用（所有権モデル）

Desktop アプリと standalone CLI は、両方インストールされている環境でも安全に共存できます。

- **共有ジョブストア**:
  すべてのジョブデータは `~/.codex-scheduler/jobs.json` に保存され、Desktop と CLI のどちらから登録・確認・キャンセルしても即座に相互反映されます。
- **単一スケジューラ常設サービス（Single Persistent Scheduler Invariant）**:
  macOS の常設スケジューラ（LaunchAgent: `dev.codexscheduler.scheduler`）はシステム全体で常に1つだけ存在します。重複起動や二重登録は発生しません。
- **Desktop 優先ルール（Ownership Priority）**:
  1. Desktop アプリがインストールされている環境では、LaunchAgent は Desktop アプリ本体（Gatekeeper 承認済みのシングル実行ファイル）が所有します。
  2. CLI 単体で先に利用していた環境（CLI 所有）に Desktop アプリを後からインストールして起動した場合、Desktop アプリが安全に所有権を引き継ぎ（Takeover）、Desktop アプリ経由でのヘッドレス定期実行へ自動移行します。
  3. Desktop 所有時に CLI から `uninstall-scheduler` を実行しても、Desktop アプリの定期実行が不意に停止しないよう安全に保護されます（アンインストールは拒否され、エラーコード `desktop_owner_protected` が返されます）。


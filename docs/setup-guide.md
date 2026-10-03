# インストールガイド

GitHub Releases からダウンロードして利用を開始するまでの手順書です。

---

## 最初に確認（前提条件）

Codex Scheduler は、指定時刻にバックグラウンドで OpenAI 公式の `codex` コマンドを実行します。  
お使いのパソコンに **OpenAI Codex CLI** がインストールされている必要があります。

ターミナル（macOS）または PowerShell（Windows）で以下を実行してください：
```bash
codex --version
```
- **成功**: `codex-cli 0.x.x` のようにバージョンが表示されます。
- **失敗**: `command not found` または `用語 'codex' は認識されません` と表示される場合は、OpenAI 公式の案内に従って先に Codex CLI をインストールしてください。

> [!IMPORTANT]
> **Windows 環境でのご注意**:  
> Windows 版 Codex Scheduler では、Codex CLI も Windows 側（PowerShell / コマンドプロンプト）にインストールされている必要があります（例: `npm install -g @openai/codex@latest`）。WSL（Windows Subsystem for Linux）内にのみインストールされた Codex CLI は本バージョンのサポート対象外です。PowerShell で `where.exe codex` を実行してパスが表示されることをご確認ください。

---

## どれを使う？（Desktop版 vs CLI版）

| 種別 | こんな方におすすめ | 特徴 |
| :--- | :--- | :--- |
| **Desktop版** | マウス操作で視覚的に管理したい方、設定を簡単に行いたい方（**迷ったらこちら**） | 直感的なGUI画面、ワンクリックでジョブ管理、OSスケジューラも自動構成 |
| **CLI版** | ターミナルやスクリプトから使いたい方、サーバーやCI環境で使いたい方 | 単一バイナリ、GUI不要で軽量、コマンドラインで全機能を操作可能 |

> [!NOTE]
> - CLI版はDesktop版をインストールしなくても単体で完結して動作します。
> - 両方をインストールして併用することも可能です（ジョブ情報は自動で共有されます）。

### 配布ファイル一覧

[GitHub Releases](https://github.com/kamahir0/codex-scheduler/releases) の最新リリースからダウンロードしてください。

| 種別 | OS / アーキテクチャ | 配布ファイル名 |
| :--- | :--- | :--- |
| **Desktop版** | macOS (Apple Silicon M1〜M4) | `Codex-Scheduler-<version>-macos-arm64.dmg` |
| **Desktop版** | Windows (64-bit) | `Codex-Scheduler-<version>-windows-x64.exe` |
| **CLI版** | macOS (Apple Silicon M1〜M4) | `Codex-Scheduler-CLI-<version>-macos-arm64` |
| **CLI版** | Windows (64-bit) | `Codex-Scheduler-CLI-<version>-windows-x64.exe` |

---

## Windows Desktop版のインストール

### 手順
1. Releases から `Codex-Scheduler-<version>-windows-x64.exe` をダウンロードします。
2. ダウンロードした `.exe` をダブルクリックしてインストーラを実行します。
3. 画面の指示に従ってインストールを完了し、アプリを起動します。

#### 「Windows によって PC が保護されました」と表示された場合
Microsoft Defender SmartScreen が表示された場合は、以下を行ってください：
1. ダイアログ内の **「詳細情報」** をクリックします。
2. 表示された **「実行」** ボタンをクリックします。

### 成功の確認
- アプリが起動し、画面上部に「Codex CLI 未検出」の警告バナーが表示されていないこと。
- 画面右上の **「新規スケジュール」** ボタンをクリックしてジョブ登録画面が表示されること。  
※ バックグラウンドスケジューラ（Task Scheduler）はジョブ登録時に自動設定されるため、追加の設定作業は不要です。

---

## macOS Desktop版のインストール

### 手順
1. Releases から `Codex-Scheduler-<version>-macos-arm64.dmg` をダウンロードします。
2. ダウンロードした `.dmg` をダブルクリックして開きます。
3. 表示されたウィンドウで、`Codex Scheduler.app` を **Applications（アプリケーション）** フォルダへドラッグ＆ドロップします。
4. Applications フォルダから `Codex Scheduler.app` を起動します。

#### 「開発元を確認できないため開けません」と表示された場合
macOS Gatekeeper によるセキュリティ確認が表示された場合は、以下のいずれかで起動できます：
- **GUI操作（推奨）**:
  1. Finderの「アプリケーション」フォルダで `Codex Scheduler.app` を **右クリック（または Control キーを押しながらクリック）** します。
  2. メニューから **「開く」** を選択します。
  3. 警告ダイアログが表示されるので、**「開く」** をクリックします（次回以降は通常起動できます）。
- **ターミナル操作**:
  ```bash
  xattr -d com.apple.quarantine "/Applications/Codex Scheduler.app"
  ```

### 成功の確認
- アプリが起動し、画面上部に「Codex CLI 未検出」の警告バナーが表示されていないこと。
- 画面右上の **「新規スケジュール」** ボタンをクリックしてジョブ登録画面が表示されること。  
※ バックグラウンドスケジューラ（LaunchAgent）はアプリ起動・ジョブ登録時に自動設定されるため、追加の設定作業は不要です。

---

## Windows CLI版のインストール

### 手順（PowerShell で実行）
以下のスクリプトを PowerShell にコピー＆ペーストして実行してください。ダウンロードフォルダにあるファイルを `%USERPROFILE%\.local\bin` に配置し、PATH を通します。

```powershell
# 1. 保存先フォルダを作成
New-Item -ItemType Directory -Force -Path "$HOME\.local\bin"

# 2. ダウンロードした実行ファイルを配置（ダウンロードフォルダにある場合）
Move-Item "$HOME\Downloads\Codex-Scheduler-CLI-*-windows-x64.exe" "$HOME\.local\bin\codex-scheduler.exe"

# 3. ユーザー環境変数 PATH に追加（現在のセッションおよび永続設定）
$bin = "$HOME\.local\bin"
$userPath = [Environment]::GetEnvironmentVariable("Path", "User")
$parts = @($userPath -split ';' | Where-Object { $_ })
if ($parts -notcontains $bin) {
    [Environment]::SetEnvironmentVariable("Path", (($parts + $bin) -join ';'), "User")
}
if ($env:Path -split ';' -notcontains $bin) {
    $env:Path = "$bin;$env:Path"
}

# 4. バージョン確認
codex-scheduler --version

# 5. OSバックグラウンドスケジューラの常設登録
codex-scheduler install-scheduler
```

> [!NOTE]
> スケジューラの登録（`install-scheduler`）は現在のユーザー権限で実行されるため、管理者権限（UAC昇格）は不要です。

### 成功の確認
以下のコマンドを実行します：
```powershell
codex-scheduler status --json
```
出力された JSON で以下が確認できれば、OSバックグラウンドスケジューラの常設登録は完了です（※ Codex CLI 自体の動作可否は、前提条件の `codex --version` で確認してください）：
- `"installed": true`（OSスケジューラに登録されている）
- `"ready": true`（実行準備完了）
- `"target_exists": true`（実行ファイルが存在する）
- `"owner_target_valid": true`（登録パスが正常）  
※ `"owner"` は通常 `"cli"` となります（Desktop版を併用している場合は `"desktop"` になることがありますが正常です）。

---

## macOS CLI版のインストール

### 手順（ターミナルで実行）
以下のスクリプトをターミナルにコピー＆ペーストして実行してください。ダウンロードフォルダにあるファイルを `~/.local/bin` に配置し、実行権限の付与とセキュリティ属性の解除を行います。

```bash
# 1. 保存先フォルダを作成
mkdir -p ~/.local/bin

# 2. ダウンロードフォルダから配置して実行権限を付与
mv ~/Downloads/Codex-Scheduler-CLI-*-macos-arm64 ~/.local/bin/codex-scheduler
chmod +x ~/.local/bin/codex-scheduler

# 3. macOS Gatekeeper（セキュリティ警告）を解除
xattr -d com.apple.quarantine ~/.local/bin/codex-scheduler 2>/dev/null || true

# 4. PATH を通す（~/.zshrc に未登録の場合）
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
  echo 'export PATH="$HOME/.local/bin:$PATH"' >> ~/.zshrc
  export PATH="$HOME/.local/bin:$PATH"
fi

# 5. バージョン確認
codex-scheduler --version

# 6. OSバックグラウンドスケジューラの常設登録
codex-scheduler install-scheduler
```

### 成功の確認
以下のコマンドを実行します：
```bash
codex-scheduler status --json
```
出力された JSON で以下が確認できれば、OSバックグラウンドスケジューラの常設登録は完了です（※ Codex CLI 自体の動作可否は、前提条件の `codex --version` で確認してください）：
- `"installed": true`（OSスケジューラに登録されている）
- `"ready": true`（実行準備完了）
- `"target_exists": true`（実行ファイルが存在する）
- `"owner_target_valid": true`（登録パスが正常）  
※ `"owner"` は通常 `"cli"` となります（Desktop版を併用している場合は `"desktop"` になることがありますが正常です）。

---

## CLI版の主なコマンド

```bash
# ジョブの新規予約（2時間後に再開）
codex-scheduler schedule --session-id "sess-123" --cwd "/path/to/project" --at "+120"

# ジョブの新規予約（日時指定・ISO 8601形式）
codex-scheduler schedule --session-id "sess-123" --cwd "/path/to/project" --at "2026-10-04T02:00:00Z"

# 登録ジョブ一覧の表示
codex-scheduler list
codex-scheduler list --json

# ジョブの詳細・実行ログの確認
codex-scheduler show <job-id>

# ジョブのキャンセル / 削除
codex-scheduler cancel <job-id>
codex-scheduler delete <job-id>

# スケジューラの稼働状態・所有権の確認
codex-scheduler status
codex-scheduler status --json
```

---

## Desktop版とCLI版を両方使う場合

Desktop版とCLI版は、同一のPC上で安心して併用できます。

- **ジョブデータの自動共有**:  
  ジョブ情報は共通の保存先（`~/.codex-scheduler/jobs.json`）で管理されます。CLIで登録したジョブはDesktop画面の一覧に即座に表示され、Desktopからキャンセルすることも可能です。
- **OSスケジューラの自動管理**:  
  バックグラウンドスケジューラはPC全体で1つだけ登録され、重複して二重起動することはありません。両方インストールされている環境ではDesktop版が自動的にスケジューラを管理します。

※ より詳しい内部仕様や権限契約については、[`docs/specs/cli.md`](specs/cli.md) および [`docs/specs/os-scheduler.md`](specs/os-scheduler.md) をご覧ください。

---

## 困ったとき（トラブルシューティング）

### 1. `codex` コマンドが見つからない
- **症状**: Desktop画面に「Codex CLI 未検出」と表示される、または `codex --version` でエラーになる。
- **対策**: OpenAI 公式の案内に従って Codex CLI をインストールしてください。また、ターミナルで `which codex`（macOS）や、PowerShell で `where.exe codex`（Windows）を実行し、実行ファイルが存在するディレクトリが環境変数 PATH に含まれているか確認してください。

### 2. macOS で「開発元を確認できないため開けません」と出る
- **対策**:
  - **Desktop版**: Finder で `Codex Scheduler.app` を右クリック（Control+クリック）し、「開く」を選択してください。
  - **CLI版**: ターミナルで `xattr -d com.apple.quarantine ~/.local/bin/codex-scheduler` を実行してください。

### 3. Windows で「Windows によって PC が保護されました」と出る
- **対策**: SmartScreen 画面の「詳細情報」をクリックし、「実行」を選択してください。

### 4. スケジューラが正常に動いているか確認したい
- **対策**:
  ターミナルまたは PowerShell で以下を実行してください：
  ```bash
  codex-scheduler status
  ```
  `Status: Ready` または `status --json` の各項目（`installed`, `ready`, `target_exists`, `owner_target_valid`）が `true` になっていれば正常に稼働しています。

### 5. ジョブ実行結果に「Not inside a trusted directory...」と出る
- **症状**: 実行履歴に `Not inside a trusted directory and --skip-git-repo-check was not specified.` と記録される。
- **対策**: Codex CLI 本体のセキュリティ保護機能により、Git リポジトリ外（ダウンロードフォルダ等）での非対話実行が制限されています。スケジュール登録時の作業ディレクトリ（`--cwd`）には、実際に作業する Git プロジェクトのフォルダを指定してください。

# 仕様: Codex プロバイダアダプタ（Codex Provider Adapter）

Status: Approved

Domain: CODEX

## 概要

OpenAI Codex CLI と連携し、指定されたセッションIDに対してプロンプト（`"continue"` 等）を非対話形式で送信・実行し、その結果（成功、失敗、利用制限枯渇）を判定する仕様を定める。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### CODEX-RESUME-001: コマンド組み立て

Codexセッションの再開コマンドは、以下の引数構造で実行されなければならない（MUST）。

```bash
codex exec resume <session_id> "<prompt>"
```

- `session_id`: 対象セッション識別子。
- `prompt`: 実行する指示文字列。

### CODEX-RESUME-002: ワーキングディレクトリ（CWD）の適用

子プロセスの実行時カレントワーキングディレクトリは、ジョブで指定された `cwd` でなければならない（MUST）。
これにより、Codexがプロジェクトローカルのコンテキスト・設定（`.codex/` や git repo）を正しく参照できるようにする。

### CODEX-RESUME-003: 非対話（Headless）実行

workerは標準入力をパイプ（`/dev/null` 相当）で閉じ、対話的プロンプトを待たずに非対話（headless）で実行しなければならない（MUST）。
これにより、深夜の自動実行がターミナル入力待ちでハングするのを防止する。

### CODEX-RESUME-004: PATH環境変数の解決およびプラットフォーム対応ランチャー解決

`launchd` や `Task Scheduler` などのOSスケジューラから起動された場合、GUIログインシェルのPATHが十分にロードされない場合がある。
Codexアダプタは、OSスケジューラからの実行時でも Codex CLI が正常に検出・起動できるよう、プラットフォームに応じたランチャー解決および PATH 補完を行わなければならない（MUST）。

1. **Windows Native におけるランチャー解決規則**:
   - ランチャー候補拡張子として `.exe`, `.cmd`, `.bat`, `.com` を探索しなければならない（MUST）。
   - 拡張子なしの POSIX shim（`codex`）は Windows native では直接起動できないため、**Windows 環境における候補から除外しなければならない（MUST NOT）**。
   - 解決優先順位は以下としなければならない（MUST）：
     1. `codex.exe`（ネイティブPE実行ファイル）
     2. `codex.cmd`（npm global shim 等のコマンドスクリプト）
     3. `codex.bat`（バッチファイル）
     4. `codex.com`
   - 探索対象ディレクトリ：
     - 現在の環境変数 `PATH`（`std::env::split_paths`）の各ディレクトリ
     - 既知の配置場所: `%APPDATA%\npm`, `%USERPROFILE%\.local\bin`, `%USERPROFILE%\.cargo\bin`
   - PATH 補完規則:
     - Windows 環境ではプラットフォーム標準の区切り文字 `;`（`std::env::join_paths`）を使用しなければならない（MUST）。
     - macOS / Linux 専用のパス（`/opt/homebrew/bin` 等）を Windows 子プロセスの PATH に混入させてはならない（MUST NOT）。

2. **macOS / Linux におけるランチャー解決規則**:
   - `codex` 単一バイナリ名を対象とし、現在の `PATH` および既知のディレクトリ（`/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `~/.local/bin`, `~/.cargo/bin`, `~/.nvm/versions/node/current/bin` 等）を探索する。
   - PATH 補完には `:` 区切り文字を使用する。

3. **ランチャー未検出時のエラー契約**:
   - 上記探索ですべての候補が見つからない場合、文字列 `"codex"` をフォールバックとして返して後段で不透明な OS エラーを発生させてはならず、明示的に `AdapterError::ExecutableNotFound("codex")` を返さなければならない（MUST）。

### CODEX-RESUME-005: Quota枯渇エラーの判定

コマンドが非ゼロの終了コードで終了した場合、標準出力（stdout）および標準エラー出力（stderr）を解析し、以下のいずれかのシグネチャに合致する場合は `is_quota_error = true` と判定しなければならない（MUST）。

- `usage limit` / `rate limit exceeded` / `rate_limit_exceeded`
- `quota exceeded` / `insufficient_quota`
- `too many requests` / `status 429` / `HTTP 429`
- `hit your usage limit`
- `credit balance is too low`

`is_quota_error = true` の場合、リトライポリシーに基づき再試行が行われる。
プロセス起動失敗（os error 193 等）や実行可能ファイル未検出エラーは Quota エラーとして分類してはならない（MUST NOT）。

### CODEX-RESUME-006: 成功判定

終了コードが `0` であり、かつ出力に致命的な未捕捉例外や認証失効が含まれていない場合、実行は成功（`succeeded`）と判定されなければならない（MUST）。

### CODEX-RESUME-007: Windows Native ランチャー境界と非スコープ

1. 本アダプタの Windows サポート境界は、**Windows native 側で利用可能な Codex ランチャー（`.cmd`, `.exe` 等）** とする。
2. WSL（Windows Subsystem for Linux）内にのみインストールされた Codex CLI の呼び出し（`wsl.exe` 経由の起動、WSL パス変換等）は本版のサポート対象外（Non-scope）とする。
3. 作業ディレクトリ（`cwd`）の Git trusted directory チェックのバイパス（`--skip-git-repo-check` の自動付与）を行ってはならない（MUST NOT）。Codex CLI 本体のセキュリティ境界を尊重する。

### CODEX-RESUME-008: 引数およびシェルメタ文字の安全な実行

1. `.cmd` / `.bat` ランチャーを実行する際、シェルインジェクションや引数境界の破損を防止するため、Rust 標準プロセスの安全なバッチ引数エスケープ規則に準拠して実行しなければならない（MUST）。
2. プロンプト文字列やセッション ID を単一のコマンド文字列へ直接文字列結合（例: `format!("cmd /c codex.cmd ... {}", prompt)`）して実行してはならない（MUST NOT）。
3. プロンプトに含まれる空白文字、ダブルクォート、および Windows 特有の記号（`&`, `|`, `<`, `>`, `^`, `%`, `!`）が別コマンドとして解釈されず、引数として安全に Codex CLI へ伝達されることを検証しなければならない（MUST）。

## 検証ルール

- `codex` コマンドが見つからない場合は `ExecutableNotFound` エラーを返さなければならない。
- Quotaエラー文字列を含むダミー出力の判定テストにより、正しく `is_quota_error` が `true` になることを検証する。
- Windows 環境において、同一ディレクトリに `codex` と `codex.cmd` がある場合に `codex.cmd` を選択することを検証する。
- Windows 環境において、空白を含む CWD および特殊記号を含むプロンプト引数が安全に子プロセスへ伝達されることを検証する。

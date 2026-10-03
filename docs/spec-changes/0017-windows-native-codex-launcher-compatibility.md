# 仕様変更提案 0017: Windows Native Codex CLI ランチャー互換性およびプロセス起動の健全化

- Status: Applied
- Date: 2026-10-03
- Author: AI Assistant (Agent-autonomous approval under Human-authorized Objective)
- Applied In: v0.5.1 candidate (target: `docs/specs/codex-adapter.md`)
- Affected Specifications:
  - `docs/specs/codex-adapter.md` (`CODEX-RESUME-004`, `CODEX-RESUME-006`, 新規 `CODEX-RESUME-007`, 新規 `CODEX-RESUME-008`)

---

## 1. 目的と背景（Why）

v0.5.0 を Windows 実環境で検証したところ、OS Task Scheduler へのジョブ登録までは正常に完了するものの、定期実行時に Codex CLI を起動する段階で以下のエラーが発生することが確認された：
```text
Failed to execute process: %1 は有効な Win32 アプリケーションではありません。 (os error 193)
```

### 原因分析
OpenAI 公式の推奨インストール方法（`npm install -g @openai/codex@latest`）を Windows で実行すると、`%APPDATA%\npm\` 配下に以下の3ファイルが生成される：
1. `codex`（POSIX シェルスクリプト shim）
2. `codex.cmd`（Windows コマンドスクリプト / cmd-shim）
3. `codex.ps1`（PowerShell スクリプト）

従来の `CodexAdapter` は Windows 上でも `dir.join("codex")`（拡張子なし）を優先探索していたため、実行不可能なテキストファイル（`codex`）を直接 `Command::new` で Win32 プロセスとして起動しようとし、`ERROR_BAD_EXE_FORMAT`（os error 193）が発生していた。
さらに、子プロセスへ渡す PATH 補完処理が macOS / Unix 向けの `:` 区切りおよび `/opt/homebrew/bin` 等の固定文字列となっており、Windows のプラットフォーム規則（`;` 区切り）に違反していた。

本変更は、Windows native 上で official npm global install で配置された `.cmd` ランチャーおよび `.exe` ネイティブバイナリを安全・確実に起動できるようにし、Windows におけるプロセス起動の健全性を保証することを目的とする。

---

## 2. 規範要件の変更（What）

### CODEX-RESUME-004: PATH環境変数の解決およびプラットフォーム対応ランチャー解決（改訂）

Codexアダプタは、OSスケジューラ（LaunchAgent / Task Scheduler）からの実行時でも Codex CLI が正常に検出・起動できるよう、プラットフォームに応じたランチャー解決および PATH 補完を行わなければならない（MUST）。

1. **Windows Native におけるランチャー解決規則**:
   - ランチャー候補拡張子として `.exe`, `.cmd`, `.bat`, `.com` を順次探索しなければならない（MUST）。
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

2. **macOS / Linux におけるランチャー解決規則（既存動作維持）**:
   - `codex` 単一バイナリ名を対象とし、現在の `PATH` および既知のディレクトリ（`/opt/homebrew/bin`, `/usr/local/bin`, `/usr/bin`, `~/.local/bin`, `~/.cargo/bin`, `~/.nvm/versions/node/current/bin`）を探索する。
   - PATH 補完には `:` 区切り文字を使用する。

3. **ランチャー未検出時のエラー契約**:
   - 上記探索ですべての候補が見つからない場合、文字列 `"codex"` をフォールバックとして返して後段で不透明な OS エラーを発生させてはならず、明示的に `AdapterError::ExecutableNotFound("codex")` を返さなければならない（MUST）。

### CODEX-RESUME-007: Windows Native ランチャー境界と非スコープ（新規）

1. 本アダプタの Windows サポート境界は、**Windows native 側で利用可能な Codex ランチャー（`.cmd`, `.exe` 等）** とする。
2. WSL（Windows Subsystem for Linux）内にのみインストールされた Codex CLI の呼び出し（`wsl.exe` 経由の起動、WSL パス変換等）は本版のサポート対象外（Non-scope）とする。
3. 作業ディレクトリ（`cwd`）の Git trusted directory チェックのバイパス（`--skip-git-repo-check` の自動付与）を行ってはならない（MUST NOT）。Codex CLI 本体のセキュリティ境界を尊重する。

### CODEX-RESUME-008: 引数およびシェルメタ文字の安全な実行（新規）

1. `.cmd` / `.bat` ランチャーを実行する際、シェルインジェクションや引数境界の破損を防止するため、Rust 標準プロセスの安全なバッチ引数エスケープ規則に準拠して実行しなければならない（MUST）。
2. プロンプト文字列やセッション ID を単一のコマンド文字列へ直接文字列結合（例: `format!("cmd /c codex.cmd ... {}", prompt)`）して実行してはならない（MUST NOT）。
3. プロンプトに含まれる空白文字、ダブルクォート、および Windows 特有の記号（`&`, `|`, `<`, `>`, `^`, `%`, `!`）が別コマンドとして解釈されず、引数として安全に Codex CLI へ伝達されることを検証しなければならない（MUST）。

---

## 3. 検証および受入条件（Verification & Acceptance）

1. **再現ユニットテスト**:
   - 同一ディレクトリに `codex`（拡張子なしテキスト）と `codex.cmd` が併存するフィクスチャにおいて、Windows リゾルバが `codex.cmd` を選択し、拡張子なし `codex` を除外することを検証。
   - `codex.exe` が存在する場合に `codex.cmd` より優先されることを検証。
   - 候補が存在しない場合に `AdapterError::ExecutableNotFound` が返ることを検証。
2. **Windows CI プロセス起動受入テスト**:
   - Windows ランナー上でダミーの `codex.cmd` フィクスチャを作成し、`CodexAdapter::execute_resume` を呼び出してプロセスが正常終了（exit 0）し、os error 193 が発生しないことを実機検証。
   - 空白を含む CWD、および記号（`&`, `|`, `%` 等）を含むプロンプト引数が正確に子プロセスへ渡ることを検証。
3. **macOS CI 回帰防止**:
   - macOS 上での既存テスト（クォータ判定、execute_tick、ジョブ実行）がすべて GREEN であることを確認。

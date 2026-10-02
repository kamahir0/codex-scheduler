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

### CODEX-RESUME-004: PATH環境変数の解決

`launchd` や `Task Scheduler` などのOSスケジューラから起動された場合、GUIログインシェルのPATH（`~/.nvm`, `~/.n`, `/usr/local/bin`, `/opt/homebrew/bin`, `~/.cargo/bin` 等）がロードされない場合がある。
Codexアダプタは、一般的なバイナリ配置パスを探索し、`codex` の絶対パスを解決するか、適切な `PATH` を補完して子プロセスを起動しなければならない（MUST）。

### CODEX-RESUME-005: Quota枯渇エラーの判定

コマンドが非ゼロの終了コードで終了した場合、標準出力（stdout）および標準エラー出力（stderr）を解析し、以下のいずれかのシグネチャに合致する場合は `is_quota_error = true` と判定しなければならない（MUST）。

- `usage limit` / `rate limit exceeded` / `rate_limit_exceeded`
- `quota exceeded` / `insufficient_quota`
- `too many requests` / `status 429` / `HTTP 429`
- `hit your usage limit`
- `credit balance is too low`

`is_quota_error = true` の場合、リトライポリシーに基づき再試行が行われる。

### CODEX-RESUME-006: 成功判定

終了コードが `0` であり、かつ出力に致命的な未捕捉例外や認証失効が含まれていない場合、実行は成功（`succeeded`）と判定されなければならない（MUST）。

## 検証ルール

- `codex` コマンドが見つからない場合は `ExecutableNotFound` エラーを返さなければならない。
- Quotaエラー文字列を含むダミー出力の判定テストにより、正しく `is_quota_error` が `true` になることを検証する。

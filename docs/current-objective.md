# Current Objective

## Objective

**Windows native Codex process launch hotfix for v0.5.1**

## Completion boundary

- Platform-aware Codex CLI launcher resolution on Windows (supporting official npm `.cmd` shim, `.exe` native binaries, excluding extensionless POSIX shims).
- Platform-correct PATH augmentation on Windows (using `;` separator, adding `%APPDATA%\npm`, `%USERPROFILE%\.local\bin`, `%USERPROFILE%\.cargo\bin`, avoiding Unix paths).
- Safe `.cmd` launcher invocation without shell injection or argument corruption.
- Preservation of working directory (`cwd`), including directories with spaces.
- Clear error contract when Codex CLI is missing (`ExecutableNotFound`).
- macOS launcher resolution regression prevention.
- Windows CI acceptance test validating actual child process launch.
- Documentation updates for Windows support boundary and troubleshooting.
- Release patch dry-run confirming v0.5.0 -> v0.5.1 readiness (stopping at Human Release Gate).

## Canonical authority

- Codex Provider Adapter specification: [`docs/specs/codex-adapter.md`](specs/codex-adapter.md) (`CODEX-RESUME-001`, `CODEX-RESUME-002`, `CODEX-RESUME-003`, `CODEX-RESUME-004`, `CODEX-RESUME-006`, `CODEX-RESUME-007`)
- OS Scheduler specification: [`docs/specs/os-scheduler.md`](specs/os-scheduler.md)
- CLI specification: [`docs/specs/cli.md`](specs/cli.md)
- Delivery specification: [`docs/specs/desktop-delivery.md`](specs/desktop-delivery.md)

## Explicit non-scope

- WSL内にしか存在しないCodex CLIの呼び出し（`wsl.exe` 経由の起動、WSLパス変換、WSL側HOME/auth/sessionDB連携）。
- Cygwin専用Codex。
- リモートLinux実行 / SSH execution backend。
- 新規Providerアダプタの追加。
- スケジューラ所有権（ownership）モデルの変更。
- Task Scheduler アーキテクチャの変更。
- `jobs.json` のスキーマ変更。
- リトライポリシー（RetryEngine）の変更（os error 193 等のプロセス起動エラーをQuota枯渇エラーと混同しない）。
- GUI機能追加 / CLIコマンド追加。
- パッケージマネージャ対応、自動更新（updater）。
- コード署名・公証。

## Status

Complete (v0.5.1 released).

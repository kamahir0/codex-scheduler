# Current Objective

## Objective

**Windows persistent scheduler + Windows standalone CLI support + cross-platform single scheduler ownership**

## Completion boundary

- Windows Desktop / CLI background scheduling (Task Scheduler 2.0 COM API, non-admin standard user compatibility)
- Single-owner cross-platform architecture (Desktop precedence, safe takeover, stale protection)
- Four official release assets (macOS DMG + standalone CLI, Windows NSIS EXE + standalone CLI)
- Released in `v0.5.0`

## Canonical authority

- OS Scheduler specification: [`docs/specs/os-scheduler.md`](specs/os-scheduler.md) (`OS-SCHED-001`, `OS-SCHED-002`, `OS-SCHED-005`, `OS-SCHED-006`)
- CLI specification: [`docs/specs/cli.md`](specs/cli.md) (`CLI-CMD-001`, `CLI-CMD-004`)
- Delivery specification: [`docs/specs/desktop-delivery.md`](specs/desktop-delivery.md) (`DELIVERY-BUNDLE-001`, `DELIVERY-BUNDLE-002`, `DELIVERY-CI-001`)
- Architecture decision record: [`docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`](adr/0004-independent-desktop-cli-single-scheduler-owner.md)
- GUI specifications: [`docs/gui/setup-and-diagnostics.md`](gui/setup-and-diagnostics.md)

## Explicit non-scope

- アプリ内自動更新（In-app updater / Tauri updater）。
- CLI 自己更新（CLI self-update）。
- Apple Developer ID / 公証（Notarization）。
- Windows コード署名証明書の購入。
- Windows Service / 常駐バックグラウンドプロセスの新設。
- ジョブごとの個別 Scheduled Task（単一タスク `CodexScheduler_Service` 不変条件を厳守）。
- リモートスケジューラ連携。
- `jobs.json` の破壊的スキーマ変更。
- ポーリング間隔（60秒）の変更。
- 新規 Provider アダプタの追加。

## Status

Completed / released in v0.5.0.

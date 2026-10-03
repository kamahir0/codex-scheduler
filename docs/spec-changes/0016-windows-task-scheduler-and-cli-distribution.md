# 仕様変更提案 0016: Windows Task Scheduler 本番バックエンド統合と Windows Standalone CLI 正式配布

- Status: Applied
- Date: 2026-10-03
- Author: AI Assistant (Agent-autonomous approval under Human-authorized Objective)
- Applied In: v0.5.0 (Candidate commit `dd3a8aa`, release tag `v0.5.0`)
- Affected Specifications:
  - `docs/specs/os-scheduler.md` (OS-SCHED-001, OS-SCHED-002, OS-SCHED-005)
  - `docs/specs/cli.md` (CLI-CMD-001, CLI-CMD-004)
  - `docs/specs/desktop-delivery.md` (DELIVERY-BUNDLE-001, DELIVERY-BUNDLE-002, DELIVERY-CI-001)
  - `docs/gui/setup-and-diagnostics.md` (DIAG-UI-001, DIAG-UI-003)
  - `docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`

---

## 1. 目的と背景（Why）

v0.4.1 において macOS では単一 LaunchAgent による常設バックグラウンド実行が確立されていたが、Windows では `FallbackScheduler` のままであり常設自動実行は未実装であった。
本変更は、Windows において Task Scheduler 2.0 COM API を用いた単一常設タスク（`CodexScheduler_Service`）による本番バックエンドを統合し、Desktop GUI と standalone CLI の双方が独立してインストール可能でありながら、共通の `jobs.json` および単一の常設スケジューラを共有するクロスプラットフォームアーキテクチャを完成させることを目的として策定された。

## 2. 採択された決定の要約（Adopted Decisions）

1. **Windows Task Scheduler 2.0 COM API 本番統合**:
   - `schtasks.exe` コマンドラインツールではなく、Task Scheduler 2.0 COM API（`ITaskService`, `ITaskFolder`, `IRegisteredTask`）を使用。
   - 非管理者（Standard User）環境において UAC 管理者昇格なしに、カレントユーザーのインタラクティブトークン（`TASK_LOGON_INTERACTIVE_TOKEN`）および最低特権（`LeastPrivilege`）で登録・稼働。
2. **単一タスク不変条件と状態遷移マトリクス（State Matrix ST-01〜ST-13）**:
   - 単一タスク名 `CodexScheduler_Service` を使用し、ジョブごとの個別タスク作成を禁止。
   - Desktop 優先（Desktop Precedence）、CLI から Desktop への安全な引き継ぎ（Safe Takeover）、無効化タスクの非破壊修復（Safe Repair）、およびターゲット消失時の非破壊保護（Stale / Invalid Protection）を規定。
3. **Windows Standalone CLI 正式配布**:
   - 配布パッケージ形式を 4 種類（Desktop 2種: macOS DMG / Windows EXE + CLI 2種: macOS arm64 / Windows x64）に正式統合。
   - 推奨配置場所: `%USERPROFILE%\.local\bin\codex-scheduler.exe`。

## 3. 承認および適用履歴（Provenance & Traceability）

- **Approval Mode**: Autonomous approval by agent under Human-authorized Objective ("Windows Task Scheduler persistent backend + Windows standalone CLI production distribution + cross-platform single scheduler ownership completion").
- **Implementation Status**: Implemented and released in `v0.5.0`.
- **Normative Authority**: 本提案の規範要件はすべて対象の canonical specifications（`docs/specs/**` および `docs/gui/**`）へ適用・統合済み。現在の規範的振る舞いは各仕様書を参照。

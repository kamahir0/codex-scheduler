# Current Objective
 
## Objective
 
**macOS Gatekeeper拒否根本修正とシングルバイナリ・ヘッドレススケジューラアーキテクチャの導入。Codex Scheduler.app の main GUI executable を 1 app / 1 executable / 2 modes（通常GUI / `--scheduler-tick` ヘッドレス）とし、別 Worker CLI へのランタイム依存を排除して LaunchAgent から直接呼出可能にする。**
 
## Completion slices
 
- **Slice 1: 仕様策定 & アーキテクチャADR**:
  - `docs/spec-changes/0013-macos-single-executable-headless-scheduler.md`: 仕様変更提案・承認記録。
  - `docs/adr/0003-macos-single-executable-headless-scheduler.md`: 1 executable / 2 modes および Gatekeeper 回避策拒否の意思決定記録。
  - `docs/specs/os-scheduler.md`, `docs/specs/desktop-delivery.md`, `docs/gui/setup-and-diagnostics.md`, `docs/setup-guide.md`: 規範仕様・ガイドの更新。
- **Slice 2: コアスケジューラ & ロック/アトミッククレーム実装**:
  - `crates/codex-scheduler-core/src/os_scheduler/macos.rs`: main executable を対象とする LaunchAgent plist 生成、launchctl エラーハンドリングの厳格化、パス変更時のみの再登録、レガシー plist クリーンアップ。
  - `crates/codex-scheduler-core/src/store.rs`: 複数 tick 間での重複実行を防ぐ atomic claim / 状態更新機構。
  - `crates/codex-scheduler-core/src/lib.rs`: 共有 `execute_tick` ロジック、`ensure_scheduler` エラーの確実な伝播。
- **Slice 3: GUI / Tauri ヘッドレスモード実装**:
  - `apps/gui/src-tauri/src/main.rs`: コマンドライン引数パースによる通常 GUI モードと `--scheduler-tick` ヘッドレスモードの分岐。
  - `apps/gui/src-tauri/src/lib.rs`: ヘッドレス tick 実行ルーチン、GUI 非表示・ウィンドウ非生成、スケジューラ登録エラーのフロントエンドへの伝播。
- **Slice 4: UI診断 & ドキュメント更新**:
  - `apps/gui/src/components/DiagnosticsModal.tsx`: スケジューラ実行パス・LaunchAgent状態診断の更新。
  - `docs/setup-guide.md`: 固定Worker分離説明の削除と、アプリ本体初回許可のみで動作する新手順の明記。
- **Slice 5: 検証 & テスト**:
  - ヘッドレスルーティング、plist 生成、冪等性、パス更新、重複実行抑止、エラー伝播のユニット/統合テスト。
  - `cargo xtask check-all` の合格。

## Authority / output routing
 
- Product vision: [`docs/product/vision.md`](product/vision.md)
- Specifications: [`docs/specs/`](specs/)
- GUI specifications: [`docs/gui/`](gui/)
- Setup guide: [`docs/setup-guide.md`](setup-guide.md)
 
## Explicit non-scope
 
- Apple Developer Program 有償アカウントの取得・公証（ad-hoc 署名環境下でのアーキテクチャ解決に集中）。
- `xattr -d com.apple.quarantine` 等の OS 保護機構のアプリ側による自動解除（禁止事項）。
- Windows タスクスケジューラの大規模リファクタ（macOS の問題解決に集中し回帰を防止）。
- Codex backend のモデル名・セッション挙動の変更。

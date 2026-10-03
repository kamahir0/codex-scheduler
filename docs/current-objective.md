# Current Objective

## Objective

**Desktop GUI版とstandalone CLI版を独立した正式distributionとして提供し、両者がshared core / shared JobStoreを利用しながら、macOSではpersistent scheduler ownerを常に1つに保ち、共存時はDesktop ownerを優先するアーキテクチャの実現。**

## Completion slices

- **Slice 1: 仕様策定 & アーキテクチャADR**:
  - `docs/spec-changes/0014-independent-desktop-cli-distributions.md`: 独立distribution、CLI command surface、macOS scheduler ownership precedence、--json output、release assetsに関する仕様変更の作成・承認記録。
  - `docs/adr/0004-independent-desktop-cli-single-scheduler-owner.md`: Desktop/CLI独立性、単一LaunchAgent維持、Desktop優先ownership precedence、Gatekeeper回避維持の意思決定記録。
  - `docs/specs/cli.md`: standalone CLIの正式canonical仕様の策定。
  - `docs/specs/os-scheduler.md`, `docs/specs/desktop-delivery.md`, `docs/gui/setup-and-diagnostics.md`, `docs/product/vision.md`, `docs/setup-guide.md`, `README.md`: 各規範仕様・ドキュメントの改定。
- **Slice 2: コアスケジューラ & Ownership管理の実装**:
  - `crates/codex-scheduler-core/src/os_scheduler/macos.rs` および `mod.rs`:
    - スケジューラ所有者（`SchedulerOwner: Desktop | Cli | None | Legacy | Invalid`）の判定ロジック。
    - 所有権優先度ルール（Desktop優先、CLIはDesktop ownerを奪わない、stale時のtakeover、Desktop起動時のCLI ownerからの安全なmigration）。
    - CLI向け LaunchAgent 登録・更新処理（`--scheduler-tick` 実行）。
    - 共有 `execute_tick` による CLI-only / Desktop 共通のジョブ実行。
- **Slice 3: standalone CLI (`codex-scheduler`) の実装**:
  - `crates/codex-scheduler-cli/Cargo.toml`: 公開バイナリ名 `codex-scheduler` の設定（`[[bin]]`）。
  - `crates/codex-scheduler-cli/src/main.rs`:
    - 正式サブコマンド（`schedule`, `list`, `show`, `cancel`, `delete`, `status`, `tick`, `install-scheduler`, `uninstall-scheduler`）。
    - `--json` フラグによる完全な機械可読JSON出力モード（成功時JSONのみ・エラー時非ゼロ終了）。
    - CLI-only 環境での自動/手動 scheduler ensure。
    - Desktop-owned scheduler の無断アンインストール防止ガード。
- **Slice 4: Desktop GUI / 診断モーダルの拡張**:
  - `apps/gui/src-tauri/src/lib.rs` (`SystemInfo`): スケジューラ所有者（`scheduler_owner`）の返却。
  - `apps/gui/src/components/DiagnosticsModal.tsx`: スケジューラ所有者（Desktop / CLI）およびステータスの表示。
  - 起動時に CLI owner が存在する場合の Desktop owner への自動 migration。
- **Slice 5: リリースパイプライン & ドキュメント更新**:
  - `.github/workflows/release.yml`: Desktop 2 assets (macOS DMG, Windows EXE) + standalone CLI 1 asset (macOS arm64 standalone binary) の計3アセットのビルド・ステージング。
  - `docs/setup-guide.md`, `README.md`: Desktop only / CLI only / 共存環境のインストール・利用ガイドの更新（macOS CLI quarantine手動解除ガイダンスを含む）。
- **Slice 6: 検証 & テスト**:
  - 所有権優先度・共存マトリクス、CLI JSON出力、相互運用性（CLI作成->Desktop実行等）のユニット・統合テスト。
  - `cargo xtask check-rationale`, `cargo xtask check-all` の合格。

## Authority / output routing

- Product vision: [`docs/product/vision.md`](product/vision.md)
- Canonical CLI specification: [`docs/specs/cli.md`](specs/cli.md)
- OS Scheduler specification: [`docs/specs/os-scheduler.md`](specs/os-scheduler.md)
- Desktop Delivery specification: [`docs/specs/desktop-delivery.md`](specs/desktop-delivery.md)
- GUI specifications: [`docs/gui/`](gui/)
- Setup guide: [`docs/setup-guide.md`](setup-guide.md)

## Explicit non-scope

- In-app updater（Tauri updater / self-update）の実装（次期以降の独立Objective）。
- HTTPサーバー / リモートリスナー / Webhook / クラウド中継等の常駐ネットワークサーバー機能の追加。
- `xattr -d com.apple.quarantine` のアプリ/CLIによる自動実行（Gatekeeper/Quarantineの手動解除ガイダンスに留める）。
- Homebrew formula、npmパッケージ、cargo-install等の外部ディストリビューション機構の構築。
- Windows Task Scheduler と CLI 単体常設スケジューラの統合（macOS ownershipモデルに集中し、Windows CLI 単体常設バックエンドは次期 Objective 候補とする）。
- `jobs.json` の破壊的スキーマ変更。
- exact-time スケジューラへの変更（OS-SCHED-005 Earliest Execution Time を維持）。

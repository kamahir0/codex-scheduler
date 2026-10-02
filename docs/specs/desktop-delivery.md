# 仕様: デスクトップ配布・リリースパッケージング（Desktop Delivery & Release Packaging）

Status: Approved

Domain: DELIVERY

## 概要

GitHub Releasesを通じて、macOS（DMG）およびWindows（インストーラ）形式でアプリケーションを配布し、エンドユーザーがダウンロード・インストールして利用できるようにするための規範仕様を定める。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### DELIVERY-BUNDLE-001: 配布パッケージ形式

Tauriバンドラーは以下のターゲット形式を生成可能でなければならない（MUST）。

- **macOS**: `.dmg`（ドラッグ＆ドロップインストール可能なディスクイメージ）および `.app` バンドル。Apple Silicon (`aarch64` / M1〜M4) 対応。
- **Windows**: `.exe`（NSIS インストーラ）および `.msi`（WiX インストーラ）。
- **Linux** (オプション): `.AppImage` / `.deb`。

### DELIVERY-BUNDLE-002: 常設Worker CLIの権限分離と固定パスプロビジョニング

頻繁に更新・置換されるGUIアプリ（`.app`）と、macOS/Windowsスケジューラからバックグラウンド実行される常設Worker（`codex-scheduler-cli`）を明確に分離しなければならない（MUST）。

1. **固定Worker配置パス（Canonical Worker Path）**:
   - macOS / Linux: `~/.local/share/codex-scheduler/bin/codex-scheduler-cli`
   - Windows: `%LOCALAPPDATA%\codex-scheduler\bin\codex-scheduler-cli.exe`
2. **自動プロビジョニング（Provisioning）**:
   - GUIアプリは初回起動時または更新時、同梱バイナリ（アプリ内Resources、同一階層、またはシステムPATH）から上記固定Worker配置パスへバイナリを配置・コピーし、Unix系OSでは実行可能権限（`0o755`）を付与しなければならない（MUST）。
   - 固定パスへの書き込みが制限される例外環境下では、検出された同梱バイナリまたはPATH上のバイナリへ安全にフォールバックしなければならない（SHOULD）。
3. **OSスケジューラ登録**:
   - `launchd` plist（macOS）および Task Scheduler（Windows）の `ProgramArguments` / コマンドラインには、必ずこの固定Workerパスの絶対パスを登録しなければならない（MUST）。GUIバイナリ自身（`current_exe`）を登録してはならない（MUST NOT）。
   - これにより、GUIアプリ本体（`.app`）をDMG経由で上書き置換・更新しても、登録済みLaunchAgentジョブの実行パスおよびWorker CLIの権限状態を永続的に維持しなければならない。

### DELIVERY-BUNDLE-003: バンドルメタデータ

`tauri.conf.json` にて以下のメタデータを設定しなければならない（MUST）。

- `productName`: `"Codex Scheduler"`
- `identifier`: `"dev.codexscheduler.app"`
- `version`: リポジトリの最新セマンティックバージョニングと一致。
- `bundle.active`: `true`
- `bundle.targets`: `["dmg", "nsis", "msi", "appimage", "deb"]`

### DELIVERY-BUNDLE-004: 完全無料（Apple Developerなし）配布とコード署名

Apple Developer Programの有償アカウントを使用しない完全無料オープンソース配布方針において、以下を満たさなければならない（MUST）。

1. **macOSコード署名**: `tauri.conf.json` の `bundle.macOS.signingIdentity` は `"-"`（ad-hoc署名）を明示指定し、Apple Siliconにおける実行拒否を防止する。
2. **権限責任の局所化**: 毎回ハッシュ値が変化しTCC権限が引き継がれないGUI本体にはファイルアクセス権限を恒久要求する処理を持たせず、固定パスに永続常駐するWorker CLI側に実行権限を寄せる。

### DELIVERY-CI-001: GitHub Actions 自動リリースパイプライン

`.github/workflows/release.yml` は以下の仕様を満たさなければならない（MUST）。

1. **トリガー条件**:
   - `git push` による `v*.*.*` タグの作成時。
   - `workflow_dispatch` による手動実行。
2. **ビルドマトリクス**:
   - `macos-latest` (macOS Apple Silicon / arm64)
   - `windows-latest` (Windows x64)
3. **ビルド手順**:
   - Node.js 環境セットアップと依存関係キャッシュ。
   - Rust ツールチェーンのセットアップ。
   - フロントエンドのビルド（`npm run frontend:build`）。
   - Tauri アプリおよび CLI バイナリのリリースビルド。
4. **リリース公開**:
   - 生成された `.dmg`, `.exe` / `.msi` ファイルを自動的に GitHub Releases のアセットとして添付・公開する。

### DELIVERY-CI-002: 成果物の命名規則

生成されるリリースアセットは、OS・アーキテクチャが明瞭に判別できるファイル名でなければならない（SHOULD）。
例:
- `Codex-Scheduler_0.1.0_aarch64.dmg`
- `Codex-Scheduler_0.1.0_x64-setup.exe`

## 検証ルール

- `tauri.conf.json` の構文および `bundle` 設定が正しくパースされることを検証する。
- `.github/workflows/release.yml` が GitHub Actions の構文要件を満たしていることを検証する。
- CLIパス解決ロジックがバンドル内外の存在を正しく検出できることをテストする。

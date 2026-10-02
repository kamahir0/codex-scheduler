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

- **macOS**: `.dmg`（ドラッグ＆ドロップインストール可能なディスクイメージ）および `.app` バンドル。Apple Silicon (`aarch64`) および Intel (`x86_64`) 両対応。
- **Windows**: `.exe`（NSIS インストーラ）および `.msi`（WiX インストーラ）。
- **Linux** (オプション): `.AppImage` / `.deb`。

### DELIVERY-BUNDLE-002: CLI Worker の同梱とパス解決

OSスケジューラ（`launchd` / `Task Scheduler`）から起動される `codex-scheduler-cli` は、エンドユーザー環境において以下の優先順序で安全に解決されなければならない（MUST）。

1. アプリケーションバンドル内のリソースディレクトリ（macOS: `Codex Scheduler.app/Contents/MacOS/codex-scheduler-cli` または `Resources/`）。
2. アプリケーション実行バイナリと同一ディレクトリ。
3. システムの `PATH`（`/usr/local/bin`, `~/.cargo/bin`, `~/.local/bin` 等）。

ジョブ登録時、GUIアプリは解決されたCLIの絶対パスをOSスケジューラに渡し、アプリ外の独立起動を保証しなければならない。

### DELIVERY-BUNDLE-003: バンドルメタデータ

`tauri.conf.json` にて以下のメタデータを設定しなければならない（MUST）。

- `productName`: `"Codex Scheduler"`
- `identifier`: `"dev.codexscheduler.app"`
- `version`: リポジトリの最新セマンティックバージョニングと一致。
- `bundle.active`: `true`
- `bundle.targets`: `["dmg", "nsis", "msi", "appimage", "deb"]`

### DELIVERY-CI-001: GitHub Actions 自動リリースパイプライン

`.github/workflows/release.yml` は以下の仕様を満たさなければならない（MUST）。

1. **トリガー条件**:
   - `git push` による `v*.*.*` タグの作成時。
   - `workflow_dispatch` による手動実行。
2. **ビルドマトリクス**:
   - `macos-latest` (macOS arm64 / x64)
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
- `Codex-Scheduler_0.1.0_x64.dmg`
- `Codex-Scheduler_0.1.0_x64-setup.exe`

## 検証ルール

- `tauri.conf.json` の構文および `bundle` 設定が正しくパースされることを検証する。
- `.github/workflows/release.yml` が GitHub Actions の構文要件を満たしていることを検証する。
- CLIパス解決ロジックがバンドル内外の存在を正しく検出できることをテストする。

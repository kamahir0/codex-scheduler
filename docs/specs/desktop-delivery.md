# 仕様: デスクトップ配布・リリースパッケージング（Desktop Delivery & Release Packaging）

Status: Approved

Domain: DELIVERY

## 概要

GitHub Releasesを通じて、macOS（DMG）およびWindows（インストーラ）形式でアプリケーションを配布し、エンドユーザーがダウンロード・インストールして利用できるようにするための規範仕様を定める。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### DELIVERY-BUNDLE-001: 配布パッケージ形式

ユーザー向け配布パッケージ形式は、一般ユーザーおよび自動化・CLIユーザーが迷わず選択できるよう、以下の4種類（Desktop 2種 + CLI 2種）に厳格に限定しなければならない（MUST）。

1. **Desktop Distribution**:
   - **macOS**: `.dmg`（ドラッグ＆ドロップインストール可能なディスクイメージ）。Apple Silicon（M1〜M4）対応。
   - **Windows**: `.exe`（NSIS インストーラ、64-bit対応）。
2. **CLI Distribution**:
   - **macOS**: standalone executable（`codex-scheduler`、Apple Silicon arm64、ad-hocコード署名済）。
   - **Windows**: standalone executable（`codex-scheduler.exe`、64-bit x64）。

#### 非公開・除外対象（Prohibited Assets）
以下の形式は、ユーザー向けRelease assetとして公開してはならない（MUST NOT）。
- `*.app.tar.gz`: 現状自動Updater機能を使用しておらず、ユーザーを混乱させるため公開禁止とする。
- `*.msi`: WiXインストーラは保守コストおよび形式重複による選択迷いを防ぐため、NSIS `.exe` に一本化し、生成・公開対象から除外する。
- Linux配布物: 現状サポート対象外。

### DELIVERY-BUNDLE-002: プラットフォーム別シングル実行ファイル構成とスケジューラ実行主体

各OSデスクトップ版において、外部の別バイナリに起因する権限・セキュリティ拒否やランタイム依存を根本防止するため、メインアプリ実行ファイル自身をOSスケジューラの実行主体としなければならない（MUST）。

1. **macOS 実行主体（1 App / 1 Executable / 2 Modes）**:
   - LaunchAgent（`dev.codexscheduler.scheduler.plist`）の `ProgramArguments` には、現在実行中のアプリ本体の絶対パス（`std::env::current_exe()`）および `--scheduler-tick` を登録しなければならない（MUST）。
   - アプリ本体が移動・更新された場合は、次回起動時またはジョブ登録時にLaunchAgent内のパス差分を検知して安全に更新しなければならない（MUST）。同一パスかつ登録済みであれば再登録を行ってはならない（MUST NOT）。
2. **Windows 実行主体（1 App / 1 Executable / 2 Modes）**:
   - Task Scheduler（`CodexScheduler_Service`）の Action には、現在実行中のアプリ本体の絶対パス（`std::env::current_exe()`）および `--scheduler-tick` を登録しなければならない（MUST）。
   - Windows Desktop は standalone CLI バイナリをランタイム依存として同梱・実行してはならない（MUST NOT depend on separate CLI binary at runtime）。
3. **Standalone CLI の独立性**:
   - CLI ツールは Desktop GUI がインストールされていない環境でも単体で自己プロビジョニング（`install-scheduler`）およびバックグラウンド予約実行を完結できなければならない（MUST）。

### DELIVERY-BUNDLE-003: バンドルメタデータ

`tauri.conf.json` にて以下のメタデータを設定しなければならない（MUST）。

- `productName`: `"Codex Scheduler"`
- `identifier`: `"dev.codexscheduler.app"`
- `version`: リポジトリの最新セマンティックバージョニングと一致。
- `bundle.active`: `true`
- `bundle.targets`: `["dmg", "nsis"]`

### DELIVERY-BUNDLE-004: 完全無料（Apple Developerなし）配布とコード署名

Apple Developer Programの有償アカウントを使用しない完全無料オープンソース配布方針において、以下を満たさなければならない（MUST）。

1. **macOSコード署名**: `tauri.conf.json` の `bundle.macOS.signingIdentity` は `"-"`（ad-hoc署名）を明示指定し、Apple Siliconにおける実行拒否を防止する。
2. **Gatekeeper 回避の禁止**: アプリケーション側から `xattr -d com.apple.quarantine` などのGatekeeper / quarantine解除コマンドを無断で自動実行してはならない（MUST NOT）。
3. **単一実行エンティティによる信頼一元化**: ユーザーが `Codex Scheduler.app` を初回に許可（Control+クリックで「開く」等）すれば、同一の実行可能ファイルがLaunchAgentからヘッドレス起動されるため、追加のMach-Oバイナリに対するGatekeeper警告を発生させずにバックグラウンド実行を完結させる。固定パス配置による「identity永続化」は保証できないためこれに依存しない。

### DELIVERY-CI-001: GitHub Actions 自動リリースパイプライン

`.github/workflows/release.yml` は以下の仕様を満たさなければならない（MUST）。

1. **トリガー条件**:
   - `git push` による `v*.*.*` タグの作成時。
   - `workflow_dispatch` による手動実行。
2. **ビルドマトリクス**:
   - `macos-latest` (macOS Apple Silicon / `aarch64-apple-darwin`)
   - `windows-latest` (Windows x64 / `x86_64-pc-windows-msvc`)
3. **ビルド手順**:
   - Node.js 環境セットアップと依存関係キャッシュ。
   - Rust ツールチェーンのセットアップ。
   - フロントエンドのビルド（`npm run frontend:build`）。
   - Standalone CLI バイナリのビルド。
   - Tauri アプリおよびインストーラのビルド。
4. **リリース公開と成果物選別**:
   - ビルド成果物から `DELIVERY-CI-002` で規定された命名規則に従ってリネームした Desktop 2種（`.dmg`, `.exe`）および CLI 2種（macOS standalone, Windows standalone `.exe`）の計4ファイルのみを明示的に選別・アップロードしなければならない（MUST）。
   - `*.app.tar.gz` や `*.msi` 等の不要な中間成果物が GitHub Releases に添付されてはならない（MUST NOT）。

### DELIVERY-CI-002: 成果物の命名規則

ユーザー向けRelease assetのファイル名は、以下の命名規則に完全準拠しなければならない（MUST）。

```text
Desktop:
Codex-Scheduler-<version>-macos-arm64.dmg
Codex-Scheduler-<version>-windows-x64.exe

CLI:
Codex-Scheduler-CLI-<version>-macos-arm64
Codex-Scheduler-CLI-<version>-windows-x64.exe
```

具体的には以下の4形式のみとする：
- macOS Desktop: `Codex-Scheduler-<version>-macos-arm64.dmg`
- Windows Desktop: `Codex-Scheduler-<version>-windows-x64.exe`
- macOS CLI: `Codex-Scheduler-CLI-<version>-macos-arm64`
- Windows CLI: `Codex-Scheduler-CLI-<version>-windows-x64.exe`

（例: `Codex-Scheduler-0.5.0-macos-arm64.dmg`, `Codex-Scheduler-CLI-0.5.0-windows-x64.exe`）

#### 命名トークン規則
- OS識別子: macOSは `macos`、Windowsは `windows-x64` を使用する（MUST）。
- アーキテクチャ識別子: Apple Siliconは `arm64` を使用する（MUST）。
- 禁止トークン: ユーザー向けasset名に `osx`, `darwin`, `aarch64`, `x86_64`, `en-US`, `setup` を使用してはならない（MUST NOT）。
- 内部ビルドターゲットとの分離: Rustのビルドターゲット（`aarch64-apple-darwin`, `x86_64-pc-windows-msvc`）は内部処理用として維持し、公開ファイル名には露出させない。

### DELIVERY-CI-003: リリース本文との完全一致

GitHub Releases のリリース本文（Release Body）に記載されるダウンロード案内・ファイル名は、実際に添付・公開されているアセットのファイル名と完全に一致していなければならない（MUST）。

## 検証ルール

- `tauri.conf.json` の構文および `bundle.targets` が `["dmg", "nsis"]` であることを検証する。
- `.github/workflows/release.yml` が GitHub Actions の構文要件を満たし、選別されたファイルのみをアップロードしていることを検証する。
- リリース本文とアセット命名規則の一致を検証する。

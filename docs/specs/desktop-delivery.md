# 仕様: デスクトップ配布・リリースパッケージング（Desktop Delivery & Release Packaging）

Status: Approved

Domain: DELIVERY

## 概要

GitHub Releasesを通じて、macOS（DMG）およびWindows（インストーラ）形式でアプリケーションを配布し、エンドユーザーがダウンロード・インストールして利用できるようにするための規範仕様を定める。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### DELIVERY-BUNDLE-001: 配布パッケージ形式

ユーザー向け配布パッケージ形式は、一般ユーザーが迷わず直感的に選択できるよう、以下の2種類のみに厳格に限定しなければならない（MUST）。

- **macOS**: `.dmg`（ドラッグ＆ドロップインストール可能なディスクイメージ）。Apple Silicon（M1〜M4）対応。
- **Windows**: `.exe`（NSIS インストーラ、64-bit対応）。

#### 非公開・除外対象（Prohibited Assets）
以下の形式は、ユーザー向けRelease assetとして公開してはならない（MUST NOT）。
- `*.app.tar.gz`: 現状自動Updater機能を使用しておらず、ユーザーを混乱させるため公開禁止とする。
- `*.msi`: WiXインストーラは保守コストおよび形式重複による選択迷いを防ぐため、NSIS `.exe` に一本化し、生成・公開対象から除外する。
- Linux配布物: 現状サポート対象外。

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
- `bundle.targets`: `["dmg", "nsis"]`

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
   - `macos-latest` (macOS Apple Silicon / `aarch64-apple-darwin`)
   - `windows-latest` (Windows x64 / `x86_64-pc-windows-msvc`)
3. **ビルド手順**:
   - Node.js 環境セットアップと依存関係キャッシュ。
   - Rust ツールチェーンのセットアップ。
   - フロントエンドのビルド（`npm run frontend:build`）。
   - Worker CLI バイナリのビルドとリソースディレクトリへのステージング。
   - Tauri アプリおよびインストーラのビルド。
4. **リリース公開と成果物選別**:
   - ビルド成果物から `DELIVERY-CI-002` で規定された命名規則に従ってリネームした `.dmg` および `.exe` の2ファイルのみを明示的に選別・アップロードしなければならない（MUST）。
   - `*.app.tar.gz` や `*.msi` 等の不要な中間成果物が GitHub Releases に添付されてはならない（MUST NOT）。

### DELIVERY-CI-002: 成果物の命名規則

ユーザー向けRelease assetのファイル名は、以下の命名規則に完全準拠しなければならない（MUST）。

```text
<Product>-<version>-<os>-<arch>.<ext>
```

具体的には以下の2形式のみとする：
- macOS: `Codex-Scheduler-<version>-macos-arm64.dmg`
- Windows: `Codex-Scheduler-<version>-windows-x64.exe`

（例: `Codex-Scheduler-0.2.2-macos-arm64.dmg`, `Codex-Scheduler-0.2.2-windows-x64.exe`）

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

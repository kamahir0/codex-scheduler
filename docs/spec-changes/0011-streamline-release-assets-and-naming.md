# 仕様変更: GitHub Release成果物の整理・命名規則の統一および不要形式の除外

Status: Applied

## Affected Specifications

- `docs/specs/desktop-delivery.md`: DELIVERY-BUNDLE-001, DELIVERY-BUNDLE-003, DELIVERY-CI-001, DELIVERY-CI-002, DELIVERY-CI-003
- `docs/setup-guide.md`: ダウンロードファイル一覧および説明
- `README.md`: ダウンロードセクション
- `apps/gui/src-tauri/tauri.conf.json`: `bundle.targets`
- `.github/workflows/release.yml`: ビルド・成果物選別リネーム・Releaseアップロード・Release本文

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - GitHub Releases の配布アセットにおいて、一般ユーザーが「自分のOS用のどれをダウンロードすればいいか」を迷わないよう、ユーザー向けRelease assetを以下の2種類のみに厳格に整理する。
    - `Codex-Scheduler-<version>-macos-arm64.dmg`
    - `Codex-Scheduler-<version>-windows-x64.exe`
  - GitHub自動生成の `Source code (zip)` および `Source code (tar.gz)` はそのまま維持する。
  - `*.app.tar.gz`（自動アップデータ用途のtar.gzは現状不要）および `*.msi`（WiXインストーラ）は公開対象から外す。
  - 命名規則を `<Product>-<version>-<os>-<arch>.<ext>` で統一し、ユーザー向けasset名には `osx`, `darwin`, `aarch64`, `x86_64`, `en-US`, `setup` を使用しない（内部ビルドターゲット `aarch64-apple-darwin`, `x86_64-pc-windows-msvc` は維持）。
  - Release本文に記載されたファイル名と実際のasset名を完全に一致させる。
- **Agent Decision**:
  - `apps/gui/src-tauri/tauri.conf.json` の `bundle.targets` を `["dmg", "nsis"]` に指定し、不要な MSI 等の生成を抑止する。
  - `.github/workflows/release.yml` において、Tauri ビルド後に生成された bundle から対象ファイルのみを選別・抽出して所定の規則でリネームし、Release に添付する方式を採用することで、Tauri内部で生成される中間成果物がReleaseに露出するリスクを排除する。

## 提案する差分（Proposed Delta）

- `docs/specs/desktop-delivery.md`:
  - `DELIVERY-BUNDLE-001`: 配布パッケージ形式を macOS (`.dmg`) と Windows (`.exe` NSIS) の2種に限定。`.msi` および `.app.tar.gz` の公開禁止を規定。
  - `DELIVERY-BUNDLE-003`: `bundle.targets` を `["dmg", "nsis"]` に更新。
  - `DELIVERY-CI-001`: リリース公開対象を明示的に選別された DMG と NSIS EXE のみに限定。
  - `DELIVERY-CI-002`: 命名規則を `<Product>-<version>-<os>-<arch>.<ext>`（`Codex-Scheduler-<version>-macos-arm64.dmg` / `Codex-Scheduler-<version>-windows-x64.exe`）に改定。禁止語句（`osx`, `darwin`, `aarch64`, `x86_64`, `en-US`, `setup`）を明記。
  - `DELIVERY-CI-003`: Release本文とアセット名の一致要件を追加。
- `docs/setup-guide.md` & `README.md`:
  - ダウンロードファイル名表記を `Codex-Scheduler-<version>-macos-arm64.dmg` および `Codex-Scheduler-<version>-windows-x64.exe` に更新。MSIへの言及を削除。
- `apps/gui/src-tauri/tauri.conf.json`:
  - `bundle.targets`: `["dmg", "nsis"]`
- `.github/workflows/release.yml`:
  - Tauri ビルド後、生成された bundle から目的のファイルを抽出してリネーム。
  - Release 本文を新しいファイル名と完全一致するように更新。
  - 明示的にリネームされた2ファイルのみを Release アセットとしてアップロード。

## 互換性（Compatibility）

- 既存の Worker CLI 同梱および固定パスプロビジョニングアーキテクチャ（`DELIVERY-BUNDLE-002`）は完全に維持される。
- 一般ユーザーに対するダウンロード導線が極めてシンプルかつ直感的になり、誤ったファイル形式のダウンロードを防止できる。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの明示的指示「採用する配布方針」に基づく)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーの明示的指示「kamahir0/codex-scheduler のGitHub Release成果物を、一般ユーザーにとって最もシンプルで分かりやすい構成へ整理してください」。
- **Status Transition**: `Proposed` -> `Approved`

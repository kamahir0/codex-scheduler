# Current Objective

## Objective

**Desktop Packaging, Release Delivery & Platform Setup。GitHub Releases から macOS用 DMG / Windows用インストーラをダウンロード・インストールし、必要なOS権限・PATH設定を経てエンドツーエンドで利用可能にするリリース配布基盤とセットアップガイド・UI診断機能を完成させる。**

## Completion slices

- **Slice 1: 仕様策定（Canonical Specs & Spec Change）**:
  - `docs/spec-changes/0002-desktop-packaging-and-release-delivery.md`: 仕様変更提案・承認記録。
  - `docs/specs/desktop-delivery.md`: Tauri バンドル設定、インストーラ生成（DMG, MSI/NSIS）、CLI worker同梱、GitHub Actions CI/CDリリースワークフロー。
  - `docs/gui/setup-and-diagnostics.md`: 初回起動時のCodex CLI検出、PATH解決、OS権限診断バナーUI。
- **Slice 2: Tauri バンドル設定 & CLIバイナリ同梱**:
  - `tauri.conf.json`: `bundle.active: true`、DMG/インストーラ設定、アイコン、メタデータ。
  - CLI worker（`codex-scheduler-cli`）をアプリバンドル内蔵または適切なパス解決で呼び出す統合設定。
- **Slice 3: GitHub Actions リリースCI/CDパイプライン**:
  - `.github/workflows/release.yml`: `v*.*.*` タグまたは手動起動で、macOS (DMG) / Windows (MSI, NSIS) のインストーラをマルチプラットフォーム自動ビルドし、GitHub Releasesにアップロード。
- **Slice 4: アプリ内診断・権限セットアップガイド & ドキュメント**:
  - GUI上に「Codex CLI 接続状況」「OSスケジューラ権限状況」の診断表示を追加。
  - `docs/setup-guide.md`: macOS（Gatekeeper/quarantine解除、LaunchAgents権限）およびWindowsのインストール・権限付与手順書。
- **Slice 5: 検証 & コミット**:
  - フロントエンド型チェック・ビルド、Rustビルド、ワークフローYAML構文検証。

## Authority / output routing

- Product vision: [`docs/product/vision.md`](product/vision.md)
- Specifications: [`docs/specs/`](specs/)
- GUI specifications: [`docs/gui/`](gui/)
- Setup guide: [`docs/setup-guide.md`](setup-guide.md)

## Explicit non-scope

- Apple Developer Program 有償アカウント必須の公的Notarization（公的署名鍵が未支給の段階では、Gatekeeper解除の公式手順書を同梱）。
- 外部独自インストーラフレームワークの自作（Tauri標準のDMG / NSIS / WiXを使用する）。

# 仕様変更: デスクトップ配布・リリースパッケージングと環境診断（Specification change: Desktop Delivery & Setup）

Status: Applied

## Affected Specifications

- `docs/specs/desktop-delivery.md`: DELIVERY-BUNDLE-001 〜 003, DELIVERY-CI-001 〜 002
- `docs/gui/setup-and-diagnostics.md`: DIAG-UI-001 〜 003
- `docs/setup-guide.md`: User-facing installation guide

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - GitHubのReleaseからOSごとのdmg（macOS）/ exeまたはmsi（Windows）等をダウンロード、インストール、必要なら権限付与、という流れでデスクトップアプリを使えるようにしたい。
- **Agent Decision**:
  - Tauri 2 の bundle 設定（DMG / NSIS / WiX）を有効化し、CLI worker バイナリの探索・同梱仕様を確立。
  - GitHub Actions `.github/workflows/release.yml` によりタグプッシュ時に macOS / Windows のバイナリ・インストーラを自動ビルド・公開。
  - アプリ起動時の環境診断（Codex CLIの有無、PATH解決）を行い、未検出時はAnt Designのアラートバナーとガイダンスを表示。
  - macOS Gatekeeper / Quarantine および Windows SmartScreen に対するインストール・権限付与ガイドを策定。

## 提案する差分（Proposed Delta）

- `docs/specs/desktop-delivery.md` 新規追加（Status: Approved）。
- `docs/gui/setup-and-diagnostics.md` 新規追加（Status: Approved）。
- `docs/setup-guide.md` 新規追加。
- Tauri設定 `tauri.conf.json` でのバンドル設定更新。
- `.github/workflows/release.yml` の新規追加。
- GUIでの環境診断バナー・モーダル追加。

## 互換性（Compatibility）

- 既存の内部APIやジョブデータ構造への破壊的変更なし。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (Human Objectiveの範囲内、Human gate なし)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Agent-autonomous
- **Basis**: ユーザーの明示的指示「githubのreleaseからosごとのdmg等をダウンロード、インストール、必要なら権限付与、と言う流れでデスクトップアプリを使えるようにしたい」。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

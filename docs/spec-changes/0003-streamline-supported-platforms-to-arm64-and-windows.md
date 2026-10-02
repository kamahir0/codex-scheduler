# 仕様変更: サポート対象プラットフォームの整理（macOS Apple Silicon & Windows x64）

Status: Applied

## Affected Specifications

- `docs/specs/desktop-delivery.md`: DELIVERY-BUNDLE-001, DELIVERY-CI-001, DELIVERY-CI-002
- `docs/setup-guide.md`: ダウンロード対象インストーラ一覧
- `README.md`: ダウンロードセクション
- `.github/workflows/release.yml`: ビルドマトリクス

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - macOS Intel (x64) の対応を削除し、開発者層の業界標準である Apple Silicon（arm64）および Windows（x64）に絞る。
- **Agent Decision**:
  - ターゲットユーザー（AIコーディングツール開発者）における Apple Silicon の圧倒的普及率（95%以上）および GitHub Actions における CI コスト・時間・キュー詰まりの削減、実機検証不能リスクの排除を理由に削除を提案・採択。

## 提案する差分（Proposed Delta）

- `docs/specs/desktop-delivery.md`: macOS のターゲットを Apple Silicon (`aarch64` / M1〜M4) に限定。
- `.github/workflows/release.yml`: `macos-x64` マトリクスエントリを削除し、ビルドジョブを `macos-arm64` と `windows-x64` の2本に集約。
- `docs/setup-guide.md` / `README.md`: Intel Mac の記載を削除し、ダウンロード導線を1つの `.dmg` に一本化。

## 互換性（Compatibility）

- 既存の Apple Silicon Mac および Windows ユーザーに対する後方互換性の影響なし。
- 配布ファイル数が削減され、ユーザーが迷わず適切なファイルを選択可能となる。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの明示的指示「削除」に基づく適用)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーの明示的指示「削除」。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

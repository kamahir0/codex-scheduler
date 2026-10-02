# 仕様変更: アプリアイコンの刷新（Custom App Icon Integration）

Status: Applied

## Affected Specifications

- `docs/gui/app-shell.md`: SHELL-BRANDING-001
- `apps/gui/src-tauri/icons/`: アプリアイコン一式（.icns, .ico, .png）
- `apps/gui/public/app-icon.png`: Webファビコンおよびヘッダーロゴ

## 根拠と分類（Source Evidence and Classification）

- **Human Decision**:
  - 提供されたアプリアイコン画像（カレンダー＋ターミナルプロンプト＋時計）を正式なアプリアイコンとして採用。
- **Agent Decision**:
  - 元画像の黒背景四隅を透過処理（Alpha channel）し、macOS Human Interface Guidelines（HIG）の角丸スクワークル（squircle）プロポーションおよび微細なドロップシャドウを合成。
  - Tauri CLI（`tauri icon`）により全解像度の `.icns`（macOS用）および `.ico`（Windows用）、PNGファイルを自動生成。
  - Webフロントエンド（`index.html` およびヘッダーUI）のプレースホルダーアイコンを本アイコンへ差し替え。

## 提案する差分（Proposed Delta）

- `apps/gui/src-tauri/icons/`: `icon.icns`, `icon.ico`, `128x128.png`, `128x128@2x.png`, `32x32.png`, `64x64.png`, `icon.png` を新デザインで再生成。
- `apps/gui/public/app-icon.png`: 高解像度マスターアイコンを追加。
- `apps/gui/index.html`: ファビコンを `/app-icon.png` に更新。
- `apps/gui/src/App.tsx`: ヘッダーの仮置きアイコンを実アプリアイコン画像に変更。

## 互換性（Compatibility）

- 既存機能・ジョブ管理ロジックへの影響なし。

## レビュー（Review）

- **Blocking Issues**: None.
- **Non-blocking Issues**: None.
- **Autonomous approval eligibility**: Yes (ユーザーの明示的指示に基づく適用)。
- **Human gate**: None.

## 承認記録（Approval Record）

- **Approval mode**: Human-directed
- **Basis**: ユーザーの明示的指示「アプリアイコンをこれにして」。
- **Status Transition**: `Proposed` -> `Approved` -> `Applied`

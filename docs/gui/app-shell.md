# GUI仕様: アプリケーションシェル（App Shell）

Status: Approved

## 目的

`codex-scheduler` の全体レイアウト、モダンなビジュアルデザイン、テーマ切り替え、および全体ステータス表示の規範仕様を定める。

## レイアウト（Layout）

Ant Design の `Layout` コンポーネントを使用し、デスクトップに最適化されたモダンなシェルを構成する。

```text
┌────────────────────────────────────────────────────────────────────────┐
│ [Logo] Codex Scheduler    [4 Jobs: 1 Scheduled, 1 Succeeded]   [+ New Job] [Theme] │ Header
├────────────────────────────────────────────────────────────────────────┤
│                                                                        │
│ ┌───────────────┐ ┌───────────────┐ ┌───────────────┐ ┌──────────────┐ │
│ │ Next Run      │ │ Active/Wait   │ │ Succeeded     │ │ Failed       │ │ Stat Cards
│ │ 02:05 AM (2h) │ │ 1 Job         │ │ 8 Jobs        │ │ 0 Jobs       │ │
│ └───────────────┘ └───────────────┘ └───────────────┘ └──────────────┘ │
│                                                                        │
│ ┌────────────────────────────────────────────────────────────────────┐ │
│ │ Job Management Table (Ant Design Table with Filters & Actions)     │ │
│ │                                                                    │ │
│ └────────────────────────────────────────────────────────────────────┘ │
└────────────────────────────────────────────────────────────────────────┘
```

1. **Header (ヘッダー)**:
   - アプリケーションロゴ & タイトル (`Typography.Title level={4}`)。
   - グローバル統計タグ (`Tag` / `Badge`)。
   - 「+ 新規ジョブ登録」プライマリアクションボタン (`Button type="primary"` with icon)。
   - ダーク / ライトモード切り替えスイッチ (`theme.darkAlgorithm` / `theme.defaultAlgorithm`)。
2. **Dashboard Summary (概要カード)**:
   - Ant Design `Card` + `Statistic` を用いた4分割メトリクス：
     - **次回実行（Next Scheduled）**: カウントダウンまたは次回予定時刻。
     - **待機中（Scheduled / Retrying）**: 実行待ちジョブ数。
     - **成功（Succeeded）**: 正常完了ジョブ数。
     - **失敗（Failed）**: 失敗ジョブ数。
3. **Content (メイン領域)**:
   - ジョブ一覧テーブルおよびフィルタリング領域。

## テーマとスタイル

- **Ant Design `ConfigProvider`**:
  - デフォルトを洗練された **Dark Theme** (`theme.darkAlgorithm`) とし、開発者に馴染みのあるダークエディタライクな配色を採用。
  - Primary Color: `#1677ff` (Ant Design Blue) または `#6366f1` (Modern Indigo)。
  - `borderRadius`: `8px` でモダンで柔らかい角丸。
  - 背景色、ボーダー、カードの陰影は Ant Design のトークンシステム（Design Tokens）を活用。

## 操作（Interactions）

- 「+ 新規ジョブ登録」ボタンをクリックすると、ジョブ作成モーダル（`Modal`）が開く。
- テーマ切り替えボタンを押すと、画面全体のテーマが瞬時に切り替わり、ローカルストレージに記憶される。
- 各種操作（作成、キャンセル、削除、手動実行）実行時は、Ant Design の `message.success` や `notification` でフィードバックを行う。

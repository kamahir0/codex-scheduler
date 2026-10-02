# GUI仕様: ジョブ管理・テーブル・ログ表示（Job Management, Table & Logs）

Status: Approved

## 目的

ジョブの新規作成フォーム、登録済みジョブ一覧の表示・検索・操作、および実行履歴ログを閲覧するための詳細仕様を定める。

## レイアウト（Layout）とコンポーネント

### 1. ジョブ一覧テーブル（Job Table）

Ant Design の `Table` コンポーネントを用い、以下のカラム構成で表示する。

| カラム名 | コンポーネント | 内容 |
| :--- | :--- | :--- |
| **Status** | `Badge` + `Tag` | 状態に応じたカラー（Scheduled: Blue, Running: Gold, Retrying: Orange, Succeeded: Green, Failed: Red, Cancelled: Default）。 |
| **Provider / Session** | `Space` + `Typography.Text` | プロバイダアイコン（Codex）とセッションID（クリップボードコピー機能付き）。 |
| **Working Directory** | `Typography.Text ellipsis` | プロジェクトパス（ホバーで `Tooltip` によるフルパス表示）。 |
| **Prompt** | `Tag` or `Text` | 送信プロンプト（デフォルト `"continue"`）。 |
| **Scheduled At** | `Typography.Text` | 予定時刻と相対時間（例: "2026-10-03 02:05 (in 2 hours)"）。 |
| **Retry** | `Progress` or `Tag` | 試行状況（例: "Attempt 1/6"、プログレスバーまたはタグ）。 |
| **Actions** | `Space` of `Button`s | 「ログ詳細（View Logs）」「今すぐ実行（Run Now）」「キャンセル（Cancel）」「削除（Delete）」 |

### 2. ジョブ登録モーダル（Job Creation Modal）

Ant Design の `Modal` と `Form` を使用し、直感的でミスのない入力体験を提供する。

1. **Provider**: `Select`（デフォルト: `OpenAI Codex`）。
2. **Session ID**: `Input`（必須。Codexデスクトップアプリ等からコピーしたIDを貼り付け）。
3. **Working Directory (cwd)**:
   - `Input` と「フォルダ選択（Browse）」ボタン（Tauri のダイアログAPIを利用してフォルダピッカーを表示）。
4. **Prompt**:
   - `Input`（デフォルト値: `"continue"`、プレースホルダ: `"continue"`）。
5. **Scheduled Time (実行日時)**:
   - `DatePicker showTime`（秒単位または分単位指定）。
   - クイック選択タグ (`Space`):
     - 「今夜 02:00（Tonight 2 AM）」
     - 「今夜 02:05（Tonight 2:05 AM）」
     - 「1時間後（In 1 Hour）」
     - 「30分後（In 30 Min）」
6. **Retry Settings (リトライ設定)**:
   - `Switch`: 利用枠（Quota）枯渇時に自動リトライする（デフォルト: ON）。
   - `InputNumber`: リトライ間隔（デフォルト: `5` 分）。
   - `InputNumber`: 最大試行回数（デフォルト: `6` 回 = 約30分間監視）。

### 3. 実行ログ詳細ドロワー（Execution Logs Drawer）

ジョブ行の「ログ詳細」ボタンをクリックすると、右側から Ant Design の `Drawer` がスライドインする。

- ジョブの基本情報要約（`Descriptions` コンポーネント）。
- 試行履歴タイムライン（`Timeline` コンポーネント）：
  - 各試行の日時、終了コード、成否、Quotaエラー判定。
- 実行ログカード：
  - ターミナル風のダーク背景ブロック（等幅フォント、`pre` スタイル）で `stdout` および `stderr` を完全表示。
  - ワンクリック「ログをコピー」ボタン。

## インタラクション

- **削除確認**: 削除ボタン押下時は Ant Design の `Popconfirm`（「このジョブを削除しますか？」）を表示し、誤操作を防止する。
- **キャンセル確認**: 待機中ジョブのキャンセル時も確認を行い、OSスケジューラからタスクを除去する。
- **即時実行（Run Now）**: スケジュール時刻を待たずに今すぐ試したい場合、ワンクリックで即座にworkerを実行可能。
- **自動リフレッシュ**: 画面表示中は定期ポーリング（5秒間隔）またはTauriイベントリスナーにより、バックグラウンドworkerの実行結果を自動検知してテーブルを更新する。

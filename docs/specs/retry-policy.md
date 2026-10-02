# 仕様: リトライポリシー（Retry Policy）

Status: Approved

Domain: RETRY

## 概要

深夜のリセット予定時刻において、一時的な利用枠枯渇（Quota Exceeded）やリセット時刻の微小なズレ（数分程度の遅延）に対応するため、一定間隔での再試行を行うリトライ制御仕様を定める。

## 用語

- [Terminology](../product/terminology.md) を参照。

## 規範要件

### RETRY-POLICY-001: リトライ設定

各ジョブのリトライポリシー（`RetryPolicy`）は、以下の属性を持つ（MUST）。

1. `enabled`: リトライを有効にするかの真偽値（デフォルト: `true`）。
2. `interval_seconds`: 再試行間隔の秒数（デフォルト: `300` 秒 = 5分、最小 30秒、最大 3600秒）。
3. `max_attempts`: 最大試行回数（初回試行含む。デフォルト: `6` 回、最大 30回）。
4. `retry_on_quota_only`: Quota枯渇エラー時のみリトライするかどうかの真偽値（デフォルト: `true`）。

### RETRY-POLICY-002: リトライ適格性の判定

試行が失敗した際、以下の条件をすべて満たす場合のみ、ジョブを `retrying` に遷移させて再試行をスケジュールしなければならない（MUST）。

1. `retry_policy.enabled == true` であること。
2. 現在の試行回数 `attempt_number < retry_policy.max_attempts` であること。
3. `retry_policy.retry_on_quota_only == true` の場合、直前の試行が `is_quota_error == true` であること。
4. ジョブがユーザーによりキャンセル（`cancelled`）されていないこと。

致命的エラー（例: 指定された作業ディレクトリが存在しない、コマンドバイナリが存在しない、認証情報が無効等）の場合は、リトライ適格外として直ちに `failed` としなければならない（MUST NOT retry）。

### RETRY-POLICY-003: 上限到達時の失敗確定

`attempt_number >= max_attempts` に達した状態で最後の試行が失敗した場合、ジョブステータスを `failed` に更新し、以降の再試行を行ってはならない（MUST NOT）。

### RETRY-POLICY-004: 成功時の即時完了

再試行のいずれかの回で実行が成功（終了コード 0）した場合、直ちにジョブステータスを `succeeded` に更新し、残りの試行をすべて停止しなければならない（MUST）。

## 検証ルール

- 初回でQuotaエラーが発生し、設定上限（例: 3回）まで再試行され、3回目でも失敗した場合はステータスが `failed` になることをテストで確認する。
- 初回でQuotaエラー、2回目で成功した場合、ステータスが `succeeded` となり3回目が実行されないことを確認する。
- 不正なディレクトリパス指定時は1回目の試行で直ちに `failed` となりリトライされないことを確認する。

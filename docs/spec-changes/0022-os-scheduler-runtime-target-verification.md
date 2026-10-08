# 仕様変更: OS スケジューラの runtime ターゲット検証とテスト分離（OS Scheduler Runtime Target Verification & Test Isolation）

Status: Applied

## Affected Specifications

- `docs/specs/os-scheduler.md` (Approved, OS-SCHED-006)
- `docs/specs/cli.md` (Approved, CLI-CMD-003)

## 根拠と分類（Source Evidence and Classification）

- **Human Evidence**:
  - Human Acceptance 実機テストにおいて、ジョブが `scheduled_at` から約7分以上経過しても開始せず `cancelled` となった。
  - 実機調査により、過去のユニットテスト実行時に一時ディレクトリ内のテスト用 plist（`/private/var/.../test-own-migrate-.../dev.codexscheduler.scheduler.plist`）がホスト実機の launchd にロードされ、一時ディレクトリ削除後に exit code 78（`EX_CONFIG`）で launchd の penalty box に入って停止していた。
  - 一方でディスク上には正規の `~/Library/LaunchAgents/dev.codexscheduler.scheduler.plist` が存在し、`launchctl list dev.codexscheduler.scheduler` コマンド自体は exit 0 を返していたため、`status --json` は `ready: true, target_exists: true, owner_target_valid: true` と誤判定（false positive）し、Desktop アプリ起動時も「既に ready」と誤認識して修復をスキップしていた。
- **Agent Decision**:
  - `OS-SCHED-006` の `Healthy` 判定条件に「ディスク上の canonical plist の期待実行ファイルと、launchd runtime に現在ロードされている実行ファイル（`Program` または `ProgramArguments[0]`）の完全一致」を必須要件として明文化する。
  - ターゲット不一致、実行ファイル消失、または runtime 状態読み取り不能の場合は `ready = false`（Unloaded / Recoverable）と判定し、fail-open を防止する。
  - Desktop / CLI の ensure および repair 処理において、runtime target 不一致（stale runtime 登録）を検知した場合、既存の stale 登録を安全にアンロード・解除した上で canonical plist を再ロードして修復する。
  - 既存の Desktop 所有権優先規則（CLI は Desktop 所有の canonical plist を上書きしない）は厳格に維持する。
  - `MacOsLaunchdScheduler` のユニットテスト・統合テストにおいて、ホスト実機の launchctl を一切変更しないよう `MockLaunchctlRunner` を必須とし、テスト実行による実機状態汚染を根絶する。

## 提案する差分（Proposed Delta）

### docs/specs/os-scheduler.md

#### OS-SCHED-006: macOS スケジューラ所有権モデルと優先度（改定）

3. **Desktop 所有スケジューラの健全性状態モデル**:
   - `Healthy`: Desktop 所有（または Cli 所有）であり、ディスク上の canonical plist に記述された対象実行ファイルパスと、launchd runtime にロードされている実行ファイルパス（`Program` または `ProgramArguments[0]`）が完全に一致し、正常に稼働可能である状態（`is_scheduler_ready() == true`）。
   - `Unloaded / Recoverable`: Desktop 所有でありディスク上に canonical plist および対象実行ファイルが実在するが、launchd runtime に未ロード、または runtime にロードされている実行ファイルが canonical plist の対象と不一致（stale runtime target）、または runtime 識別子が読み取り不能である状態（`is_scheduler_ready() == false`）。
   - `Malformed / Invalid`: 登録 plist の構文不正、未対応ラベル、または登録実行ファイルがディスク上に存在しない状態。

4. **優先度および修復ルール（Precedence & Repair Rules）**:
   - **stale runtime 登録の安全修復（Safe Runtime Stale Repair）**: `Unloaded / Recoverable` 状態（runtime 未ロードまたは runtime target 不一致）のスケジューラが存在する場合、CLI または Desktop からの ensure / repair 要求は、canonical plist の内容（Desktop 所有時は Desktop 実行ファイル）を維持したまま、stale な runtime 登録を安全にアンロード／解除し、canonical plist を `launchctl load -w` で再ロード修復しなければならない（MUST）。
   - **Desktop 優先（Desktop Precedence）の維持**: CLI からの repair において、Desktop 所有の canonical plist を CLI 実行ファイルで上書きしてはならず（MUST NOT）、Desktop 所有権および正規の Desktop 実行ファイルパスを保持したまま launchd runtime を修復しなければならない（MUST）。

### docs/specs/cli.md

#### CLI-CMD-003: status コマンドの JSON 出力（改定）

- `scheduler.ready`: OS スケジューラが正常にロードされ、かつ runtime にロードされている実行ファイルがディスク上の canonical 設定と一致している場合のみ `true`、不一致・未ロード・エラー時は `false`（MUST）。

## 互換性（Compatibility）

- 既存の `status --json` スキーマ（フィールド名および型）に対する breaking change はなく、完全な後方互換性を維持する。
- 既存の Desktop / CLI 所有権移行ルールおよび優先規則を破壊せず、不整合検出と自己修復精度を向上させる。

## 受け入れと実装への影響（Acceptance and Implementation Impact）

- ユニットテストおよび統合テスト実行前後で、実ホストの launchctl 登録状態が変化しないこと。
- runtime target == expected target の場合のみ `ready = true`。
- runtime target != expected target の場合 `ready = false`。
- runtime target 不一致時に ensure / repair で canonical plist を保持したまま launchd を再ロード修復できること。
- CLI repair 時に Desktop 所有 canonical plist が上書きされないこと。

## 未解決事項（Open Questions）

- なし。

## レビュー（Review）

- **Blocking**: なし。
- **Approval Eligibility**:
  - Human が選択した Current Objective（Long-running Codex execution lifecycle）の完了境界（Scheduler availability / Single-writer safety / Verification）に必要なスコープ内。
  - Human gate 条件（仕様全体の変更、非互換性変更、外部破壊的操作）に該当しない。
  - non-breaking bugfix / hardening であり、testable な検証条件が定義されている。
- **Review Result**: Autonomous approval 条件を満たす。

## 承認記録（Approval Record）

- **Approval Mode**: Autonomous (Agent delegated under AGENTS.md and docs/contributing/specification-workflow.md)
- **Basis**: Human Acceptance failure（実機 launchd 汚染および false positive 判定）の解決に閉じた必要最小限の correction。
- **Review Result**: Pass (Approved)
- **Canonical Application**: Applied to `docs/specs/os-scheduler.md` and `docs/specs/cli.md`.

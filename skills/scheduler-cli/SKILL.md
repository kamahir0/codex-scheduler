---
name: scheduler-cli
description: Use the standalone codex-scheduler CLI from an AI agent to schedule Codex session resumes, inspect and manage jobs, and verify scheduler health safely on macOS and Windows.
---

# scheduler-cli

## 目的

AI agent が `codex-scheduler` CLI を機械可読な形で安全に操作し、Codex セッションの予約再開、ジョブ確認、キャンセル、削除、スケジューラ健全性確認を行うための procedure。

observable behavior の owner は [CLI specification](../../docs/specs/cli.md) と [OS Scheduler specification](../../docs/specs/os-scheduler.md)、導入・更新手順の owner は [Setup Guide](../../docs/setup-guide.md) であり、この skill はそれらを置き換えない。

## 発動トリガー

次の依頼ではこの skill を使う。

- 「この Codex セッションを2時間後に再開して」のような予約実行。
- quota / rate limit の解除後に現在の作業を再開する予約。
- 既存ジョブの一覧、詳細、状態、キャンセル、削除。
- Desktop / CLI 共存環境で scheduler ownership / health を確認してから予約する自動化。

CLI のインストール、アップデート、release、製品開発自体が目的なら、それぞれの canonical procedure を使う。

## Agent defaults

- 自動化では `--json` を優先し、stdout の JSON と exit code を判定に使う。
- 通常の予約では `tick` / `--scheduler-tick` を手動実行しない。OS scheduler に実行を委譲する。
- implementation 上に存在する非公開/debug commandを通常運用へ持ち込まない。
- session ID、実行時刻、対象 project を推測で発明しない。trusted runtime context または Human 指示から確定できなければ確認する。
- `--skip-git-repo-check` 等、Codex 側の trust boundary を弱める引数を自動追加しない。
- 予約後に agent process を時刻まで常駐させない。正常な OS scheduler 登録を確認したら終了してよい。
- Desktop ownership を尊重し、CLI から ownership を奪わない。

## Preflight

まず CLI と Codex CLI の存在を確認する。

```bash
codex-scheduler --version
codex --version
codex-scheduler status --json
```

Windows native 環境では必要に応じて次も確認する。

```powershell
where.exe codex
```

Windows では Windows 側の Codex CLI が必要であり、WSL 内にしか存在しない Codex は current support boundary 外である。

`status --json` は canonical spec の health predicate に従って判定する。

```text
installed == true
ready == true
target_exists == true
owner_target_valid == true
```

`owner == "desktop"` の場合に `path_matched == false` でも正常。CLI executable と Desktop executable が異なるためであり、failure にしない。

`owner == "invalid"` / `"legacy"`、または scheduler が安全に ready にならない場合は、上書き修復を発明せず状態とエラーを Human へ返す。

CLI 自体が未導入なら [Setup Guide](../../docs/setup-guide.md) へrouteする。通常の予約taskだけを理由に、勝手にバイナリをダウンロード・更新しない。

## Scheduling input の決定

### session-id

再開対象の Codex session ID を使う。current agent/runtime が authoritative な session ID を提供している場合はそれを使える。分からない場合は Human に確認し、別sessionのIDを推測しない。

### cwd

実際に作業を再開する project directory の絶対pathを使う。Git project の場合は必要に応じて:

```bash
git rev-parse --show-toplevel
```

でrootを確定する。

Codex が trusted repository error を返した場合は `cwd` を正しい project へ直す。trust check を無効化して回避しない。

### at

Human が指定した時刻をそのまま最早開始時刻として扱う。

利用可能な形式は canonical CLI spec に従い、たとえば:

```text
+120
2026-10-04T02:00:00Z
2026-10-04 11:00
```

相対指定で即時に近い予約が必要でも `+0` に依存せず、通常は少なくとも `+1` 以上の未来を使う。

### prompt / retry

prompt 指定がなければ CLI の default `"continue"` を利用する。Human が具体的な再開指示を与えた場合のみ `--prompt` でその内容を渡す。

retry policy は特段の要件がなければ CLI default を維持する。one-shot の検証等で retry を望まない明確な理由がある場合だけ `--max-attempts 1` 等を指定する。

## 通常の予約flow

1. preflight。
2. session ID / cwd / at を確定。
3. `schedule --json`。
4. 戻り値の job ID を保持。
5. `show --json` と `status --json` で予約と scheduler health を確認。
6. Human へ job ID、時刻、cwd、scheduler owner を報告して終了。

例:

```bash
codex-scheduler schedule --session-id "<session-id>" --cwd "<absolute-project-path>" --at "+120" --json
codex-scheduler show "<job-id>" --json
codex-scheduler status --json
```

prompt を明示する場合:

```bash
codex-scheduler schedule --session-id "<session-id>" --cwd "<absolute-project-path>" --at "<time>" --prompt "<instruction>" --json
```

`schedule` 自体が必要な scheduler ensure を行うため、成功した予約の直後に `install-scheduler` を重ねて実行する必要はない。

## Ownership safety

`status --json` の `scheduler.owner` で分岐する。

### desktop

- healthy ならそのまま `schedule` する。
- `path_matched == false` を異常扱いしない。
- `install-scheduler` で CLI ownership へ切り替えようとしない。
- `uninstall-scheduler` を実行しない。

### cli

- healthy なら通常利用する。
- scheduler 登録の明示修復を Human が求めた場合のみ `install-scheduler --json` を使う。
- uninstall は Human の明示意図があり、owner が本当に `cli` であることを確認してから行う。

### none

通常は `schedule` に自己プロビジョニングを任せる。schedule 成功後に `status --json` で health predicate を再確認する。

### legacy / invalid

自動上書きや ownership takeover を行わない。status と stderr を保持して Human に返す。

## Job inspection

一覧:

```bash
codex-scheduler list --json
```

詳細・execution history:

```bash
codex-scheduler show "<job-id>" --json
```

- 実行中（`status == "running"`）のジョブでは、戻り値の JSON に `active_execution`（`is_runner_active`, `codex_process_alive`, `liveness_state`, `attempt_number`, `log_path`, `latest_output`）が含まれる。
- `liveness_state`（`"runner_active"` | `"child_active"` | `"unknown"`）により、Runner プロセスだけでなく Codex 子プロセスの生存状況を正確に判定できる。
- 実行中の進捗確認は、`show --json` の `active_execution.latest_output` または `log_path` の read-only 読み取りによって行う。
- **状況確認を目的として、同一 Codex セッションに対して追加の `codex resume` や `codex exec resume` を別プロセスとして起動してはならない（MUST NOT）**。

期限前の `Scheduled`、quota retry 中の `Retrying` はそれぞれ通常状態になり得る。実行結果の Codex 側エラー（auth / session / quota / network / repository trust 等）と scheduler health failure を混同しない。

## Cancel / delete

キャンセル:

```bash
codex-scheduler cancel "<job-id>" --json
```

削除:

```bash
codex-scheduler delete "<job-id>" --json
```

mutation 前に `show --json` で exact job ID と状態を確認する。
**実行中（`Running` または active execution 保持中）のジョブに対しては、同一セッションへの二重 writer 事故を防止するため `cancel` および `delete` は拒絶されエラーとなる。**

`delete` は履歴を含むジョブを消すため、Human が削除を求めた場合、または current task で agent 自身が作成した一時検証jobの cleanup が明確な場合に限定する。通常の予定変更では、対象を確認して `cancel` してから新しいjobを `schedule` する。

## Failure routing

- `schedule` が非ゼロ終了した場合、成功したjobが作られたと推測しない。stderr / structured error を返す。
- scheduler health が不正なら scheduler 設定を無断で書き換えない。
- Codex executable が見つからない場合は Codex CLI prerequisite の問題として扱う。
- Windows で extensionless shim / WSL-only path 等に戻す workaround を作らない。
- quota error で `Retrying` になった場合は configured retry policy に任せ、agent が同じjobを重複作成しない。
- due time 後も実行されない場合は `show --json` と `status --json` を取得し、job state と scheduler health を分けて報告する。

## Completion report

予約taskでは最低限次を返す。

```text
Scheduled
- Job ID:
- Session ID:
- CWD:
- Earliest execution time:
- Scheduler owner:
- Scheduler healthy: yes/no
```

失敗時は command、exit status、stderr / structured error、scheduler owner / health を簡潔に報告する。sessionの本文や不要な機密情報をログ目的で複製しない。

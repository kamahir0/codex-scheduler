# Current Objective

## Objective

**Long-running Codex execution lifecycle**

Schedulerによってquota解除後などにCodex sessionを`continue`で再開した際、Codexの実作業が数十分〜数時間継続しても、OS scheduler tickを長時間占有せず、jobを正しくRunningとして管理し、実際のCodex execution終了時にだけSucceeded / Failed / Retryingを確定できる実行ライフサイクルを実現する。

## Completion boundary

- **Scheduler tickの分離**: due jobのclaimとlong-running Codex executionを分離し、OS scheduler tickプロセスが長時間ブロックされず短時間で終了可能にする。
- **Running lifecycle**: 正常に起動されたlong-running process実行中はRunningを維持し、stdout/stderrの未完結や通常progress出力をfailureとせず、実プロセス終了時にのみSucceeded / Failed / Retryingを確定する。
- **Scheduler availability**: 1つのlong-running jobによって後続のscheduler tickや別due jobの処理が恒常的に妨げられないようにし、同一jobの二重実行を防止する。
- **Crash / orphan recovery**: runnerプロセスの異常終了・マシン再起動時にjobが永久にRunningに残らないようにしつつ、生存している長時間プロセスをstaleと誤判定して二重実行しない。
- **stdout / stderrの安全な保持**: メモリ無制限蓄積やjobs.json肥大化を防止するbounded/safeなログ保持を行い、実行中の状況をread-onlyで確認可能にする。
- **Single-writer safety**: monitoring目的での追加resumeや二重writer起動を防止し、active writer競合を適切に扱う。
- **Shared core**: Desktop / CLI共通で`codex-scheduler-core`の同一execution semanticsを使用し、macOSとWindowsで同等のライフサイクルを提供する。
- **Compatibility**: 既存`jobs.json`のbackward-compatibleな読み込みと既存CLI互換性を維持する。
- **Verification**: controllable fake/helper processを用いた短時間・決定論的テスト、macOS/Windows CI、`cargo xtask check-rationale`、`cargo xtask check-all`の成功。

## Canonical authority

- ジョブライフサイクル仕様: [`docs/specs/job-lifecycle.md`](docs/specs/job-lifecycle.md)
- Codex Adapter仕様: [`docs/specs/codex-adapter.md`](docs/specs/codex-adapter.md)
- OS Scheduler仕様: [`docs/specs/os-scheduler.md`](docs/specs/os-scheduler.md)
- Retry Policy仕様: [`docs/specs/retry-policy.md`](docs/specs/retry-policy.md)
- CLI仕様: [`docs/specs/cli.md`](docs/specs/cli.md)

## Explicit non-scope

- Codex Desktopとrunning session間のwriter takeover/handoff。
- Schedulerから実行中Codex taskをinteractiveに操作するTUI。
- 一般目的のprocess supervisor / daemon framework。
- HTTP/WebSocket serverの新設。
- plugin framework。
- Codex sessionそのものの内部protocolの再実装。
- quota exhaustionを実際に数時間待って再現するCI。
- release / tag / GitHub Release publication。
- 実行中jobプロセスの強制終了（cancel semantics拡張）。

## Status
 
In progress.

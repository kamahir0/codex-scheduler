# Development State

Stage: verification-ready
Candidate: ac9c2858ae31f8f9024f2161f3653ea954cfcb64
Work base: 67710c63fd2257f3248aee2f7804a267aed5d0d1

## Active work

- Current candidate: ac9c2858ae31f8f9024f2161f3653ea954cfcb64
- Human Acceptance failure correction and verification completed:
  1. Spec-change `0022-os-scheduler-runtime-target-verification.md` applied to `docs/specs/os-scheduler.md` (OS-SCHED-006) and `docs/specs/cli.md` (CLI-CMD-003).
  2. Isolated unit tests from real launchd: `RealLaunchctlRunner` panics on tests; `with_dir` defaults to `MockLaunchctlRunner`. Real host launchd is never mutated by test suites.
  3. Strict runtime target verification in `is_scheduler_ready`: parses `launchctl list` stdout and matches loaded executable against canonical plist target. Fails closed on mismatch, missing target, or unreadable identity.
  4. Safe repair on ensure/repair: clears stale same-label runtime registration before reloading canonical Desktop plist.
  5. Verified across full suite: 33 macOS scheduler unit tests, 68 core tests, 7 integration tests, 48 lifecycle tests, 10 xtask tests, `cargo xtask check-rationale`, `cargo xtask check-all`, Windows MSVC cross-compilation.
  6. Real host repaired and verified:
     - launchctl print: runtime program == `/Applications/Codex Scheduler.app/Contents/MacOS/codex-scheduler-gui`, last exit code = 0, penalty box removed.
     - status --json: installed=true, ready=true, owner=desktop, target_exists=true, owner_target_valid=true.
  7. Human Acceptance job `8a157c68-2c3e-4ecb-8d88-33803150a9f6` scheduled at 11:53:28Z and automatically claimed & executed on next launchd tick (11:54:07Z, run 3).
- Next: Human Acceptance evaluation

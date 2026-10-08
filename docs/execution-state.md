# Development State

Stage: correction-ready
Candidate: 24903f1df73dc6dc2348fa95bce711f6b64e06bf
Work base: 67710c63fd2257f3248aee2f7804a267aed5d0d1

## Active work

- Current candidate: 24903f1df73dc6dc2348fa95bce711f6b64e06bf
- Human Acceptance failure correction:
  - Blocker 1: macOS unit tests mutate real launchd (with_dir uses RealLaunchctlRunner; stale test plist remained in launchd)
  - Blocker 2: scheduler readiness false positive (is_scheduler_ready only checks exit status of launchctl list, not matching runtime-loaded target with disk plist target)
- Required actions:
  1. Spec-change for OS-SCHED-006 runtime target verification
  2. Isolate macOS unit tests with MockLaunchctlRunner (no side effects on real launchd)
  3. Implement runtime target extraction and matching in is_scheduler_ready
  4. Ensure safe repair of same-label stale runtime registration on ensure/repair
  5. Run all required tests and xtasks
  6. Repair real host launchd and verify with status --json
  7. Schedule and verify new Human Acceptance job

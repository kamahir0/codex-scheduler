# Current Objective

## Objective

**Job Table UI minor fixes: Copy button wrap prevention and descending scheduled time sort**

## Completion boundary

- Prevent copy button wrapping in the "Session ID / Provider" column of the Job Table across different screen resolutions and OS font rendering environments (apply nowrap and flex-wrap prevention).
- Set default sort order of the "Scheduled At" column in the Job Table to descending (`descend`), displaying jobs with later scheduled times at the top by default.
- Maintain existing responsiveness, horizontal scroll (`scroll={{ x: ... }}`), and column layout invariants.
- Verification via `cargo xtask check-all` (frontend lint, frontend build, core tests, rationale).

## Canonical authority

- GUI Job Scheduling specification: [`docs/gui/job-scheduling.md`](docs/gui/job-scheduling.md)
- App Shell specification: [`docs/gui/app-shell.md`](docs/gui/app-shell.md)

## Explicit non-scope

- Backend/Rust core API, scheduler, or store modifications.
- CLI argument or command modifications.
- Additional table columns or styling overhauls unrelated to the session copy button and default sort order.

## Status

In progress.

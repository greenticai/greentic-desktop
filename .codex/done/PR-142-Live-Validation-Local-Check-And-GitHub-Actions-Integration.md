# PR-142 - Live Validation Local Check And GitHub Actions Integration

## Goal

Integrate live validation into developer workflows without making normal CI depend on a graphical desktop session.

## User Outcome

Developers can run the real desktop validation suite locally, while GitHub Actions continues to run deterministic checks and reports when live validation was skipped.

## Current Evidence

- `local_check.sh` and GitHub Actions have repeatedly diverged.
- CI cannot open local Word/Excel, but local checks need an explicit live mode so desktop automation does not regress unnoticed.
- Passing model tests gave false confidence that the product worked.

## Scope

1. Add `ci/live_desktop_check.sh`.
2. Add environment gates:
   - `GREENTIC_LIVE_DESKTOP_TESTS=1`.
   - `GREENTIC_LIVE_SUITE=macos|windows|linux|web|terminal|all`.
   - `GREENTIC_LIVE_KEEP_ARTIFACTS=1`.
3. Update `ci/local_check.sh`:
   - always run deterministic tests.
   - print a clear warning when live desktop validation is skipped.
   - run live validation when the env var is set.
4. Update GitHub Actions:
   - do not fail because live desktop validation is skipped on headless runners.
   - optionally run web/terminal live fixtures that do not require a GUI.
5. Add a summary artifact format:
   - JSON summary.
   - Markdown summary.
   - evidence directory.
6. Make live validation failures non-ambiguous:
   - setup missing.
   - app missing.
   - permission missing.
   - workflow failed.
   - assertion failed.

## File Targets

- `ci/live_desktop_check.sh`
- `ci/local_check.sh`
- `.github/workflows/*`
- `docs/live-validation.md`

## Out of Scope

- Installing Office in GitHub-hosted CI.
- Running GUI automation in headless Linux unless an explicit supported fixture exists.

## Acceptance Tests

1. `bash ci/local_check.sh` prints that live validation is skipped unless `GREENTIC_LIVE_DESKTOP_TESTS=1`.
2. `GREENTIC_LIVE_DESKTOP_TESTS=1 bash ci/local_check.sh` runs the live suite appropriate to the host OS.
3. GitHub Actions remains green without a graphical desktop.
4. The CI summary clearly distinguishes deterministic checks from live desktop validation.
5. Live suite evidence is written to `target/greentic-live-validation/`.

## Done Means

Local development has a real desktop validation path, and CI no longer creates false confidence by silently omitting it.

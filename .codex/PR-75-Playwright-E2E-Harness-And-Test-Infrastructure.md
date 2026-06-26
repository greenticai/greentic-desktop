# PR-75 - Playwright E2E Harness and Test Infrastructure

## Goal

Add a reliable Playwright-based end-to-end harness that can launch `greentic-desktop`, open the real Automate Hub GUI, call the local API with the GUI session token, capture logs/screenshots/traces, and run fast deterministic tests in CI plus optional permissioned desktop tests locally.

## Problem

Current validation is mostly Rust unit/integration tests plus a shell smoke test. That catches API and model regressions, but it does not prove that the browser GUI, local API token handling, frontend workflows, setup actions, runner creation, run/test actions, MCP publishing, and desktop automation flows work together as a product.

## User Outcome

After this PR, every functional test starts the same way a user starts Greentic Desktop:

```bash
cargo run --bin greentic-desktop -- gui --no-open --bind 127.0.0.1:0
```

The Playwright harness then reads the printed URL, preserves the token, opens the GUI, performs user actions, and fails with actionable artifacts when behavior regresses.

## Scope

1. Add Playwright as a frontend dev dependency.
2. Add `frontend/automate-hub/playwright.config.ts`.
3. Add `frontend/automate-hub/e2e/` test folder.
4. Add a reusable test fixture that:
   - builds or locates `target/debug/greentic-desktop`
   - starts it with a fresh temporary `GREENTIC_DESKTOP_HOME`
   - parses `Greentic Automate Hub: http://127.0.0.1:<port>/?token=<token>`
   - opens the URL in Playwright
   - exposes API helpers with the same token
   - captures stdout/stderr to a test artifact
   - kills the process after each test
5. Add deterministic fixture controls through environment variables or API-only test hooks:
   - fake permission mode: all granted, all missing, macOS screen capture missing
   - fake LLM response mode
   - fake recorder backend mode
   - fake native desktop app mode where real OS permissions are not available
6. Add `npm run e2e` and `npm run e2e:headed`.
7. Add `ci/gui_e2e.sh` wrapper.
8. Add CI job that runs the non-permissioned Playwright suite on Linux.
9. Add local commands for permissioned macOS/Windows/Linux tests but keep them out of mandatory CI unless the runner is explicitly prepared.

## Test Harness Contract

Each Playwright test must get:

- `page`: browser page opened to the live GUI URL
- `api`: helper with `get`, `post`, `put`, `patch`, and token header
- `runtimeHome`: unique temporary runtime folder
- `processLogs`: path to captured GUI logs
- `expectNoRedErrors()`: helper asserting no global error toast remains
- `snapshot(label)`: helper that stores screenshot and relevant API state

## Required Artifacts on Failure

Store these in Playwright output:

- screenshot
- browser console log
- network failures
- GUI stdout/stderr
- runtime home tree summary
- last `/api/v1/health`
- last `/api/v1/setup/checklist`
- if a runner exists, `/api/v1/runners` and latest evidence bundle summary

## Test Tags

Use tags so CI and local runs can select the right scope:

- `@smoke`: must run quickly in every PR
- `@functional`: normal deterministic E2E
- `@llm-mock`: uses fake LLM responses
- `@desktop-fake`: uses fake desktop app backend
- `@desktop-real`: requires real OS permissions and real apps
- `@macos`, `@windows`, `@linux`
- `@manual`: not run in CI

## CI Integration

Add to `ci/local_check.sh` behind an opt-in first:

```bash
GREENTIC_CHECK_E2E=1 ./ci/local_check.sh
```

Then make the fast smoke subset mandatory once stable:

```bash
npm --prefix frontend/automate-hub run e2e -- --grep @smoke
```

## Acceptance Criteria

- Playwright can launch the real Rust GUI host and load the GUI with token handling.
- Tests fail if the token is missing from mutating API calls.
- Tests collect screenshots, traces, logs, and runtime-state artifacts.
- CI can run `@smoke` tests without real screen recording permissions.
- Local developer command can run all deterministic Playwright tests.
- Permissioned desktop tests are clearly separated and skipped unless explicitly requested.

## Test Plan

- `npm --prefix frontend/automate-hub run build`
- `npm --prefix frontend/automate-hub run e2e -- --grep @smoke`
- `cargo test -p greentic-desktop-gui`
- `GREENTIC_CHECK_E2E=1 ./ci/local_check.sh`

## Risks

- Browser install size and runtime can slow CI. Use Playwright cache and run Chromium only for the mandatory suite.
- Real desktop automation is sensitive to OS permissions. Keep fake/deterministic flows mandatory and real OS flows optional until dedicated runners exist.
- Tokenized localhost URLs must not be logged in public CI output except in private job artifacts.

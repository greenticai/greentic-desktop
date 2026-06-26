# PR-84 - Playwright Release Gate and Cross-Platform Matrix

## Goal

Turn the Playwright suites into a clear release gate so `greentic-desktop` is considered functional only when the right tests are green for the right platform/environment.

## Problem

Some tests can run everywhere, while others require OS permissions, GUI sessions, JDKs, or real desktop apps. Without a matrix and gating policy, tests either become too weak to matter or too brittle to keep green.

## Test Matrix

| Suite | Linux CI | macOS CI | Windows CI | Local Manual | Required for Merge |
| --- | --- | --- | --- | --- | --- |
| GUI smoke/setup | yes | yes | yes | yes | yes |
| Extension install/remove | yes | yes | yes | yes | yes |
| LLM mock prompt runner | yes | yes | yes | yes | yes |
| Web fixture automation | yes | yes | yes | yes | yes |
| Recording fake backends | yes | yes | yes | yes | yes |
| MCP publish/run/delete | yes | yes | yes | yes | yes |
| Native fake calculator | yes | yes | yes | yes | yes |
| Real macOS Calculator | no | optional prepared runner | no | yes | no |
| Real Windows Calculator | no | no | optional prepared runner | yes | no |
| Real Linux X11 app | optional xvfb | no | no | yes | no |
| Java fake fixture | yes | yes | yes | yes | yes if JDK cached |
| Real Java accessibility | no | optional | optional | yes | no |
| Real LLM provider | no | no | no | yes with secrets | no |

## Scope

1. Add CI workflows:
   - `gui-e2e-smoke.yml`
   - `gui-e2e-functional.yml`
   - optional `gui-e2e-desktop-manual.yml` with workflow dispatch
2. Add scripts:
   - `ci/gui_e2e_smoke.sh`
   - `ci/gui_e2e_functional.sh`
   - `ci/gui_e2e_desktop_manual.sh`
3. Add Playwright projects:
   - `chromium-smoke`
   - `chromium-functional`
   - `desktop-real-macos`
   - `desktop-real-windows`
   - `desktop-real-linux`
4. Add release readiness report:
   - command prints which suites passed
   - shows skipped manual suites and why
   - links artifacts
5. Add `local_check.sh` integration:
   - smoke mandatory after stabilization
   - functional optional with `GREENTIC_CHECK_E2E=1`
   - desktop manual only with explicit env flags
6. Add documentation:
   - `docs/testing-e2e.md`
   - permission setup for real desktop tests
   - secrets setup for real LLM tests
   - troubleshooting for flaky OS permission states

## Green Definition

A PR is mergeable when:

- Rust tests pass.
- Frontend build passes.
- Playwright smoke suite passes.
- Playwright functional mock suite passes for changed areas or nightly.
- No test uses real public websites in required CI.
- No test requires a real LLM secret in required CI.

A release is publishable when:

- all merge gates pass on main
- nightly functional suite passes
- at least one prepared machine has run the real desktop smoke for each supported OS during the release cycle, or the release notes call out unsupported status

## Acceptance Criteria

- CI clearly separates mandatory deterministic tests from optional real-environment tests.
- Required tests are stable and do not depend on public websites, real LLMs, or desktop permissions.
- Manual real-environment test commands are documented and easy to run.
- Release gate report makes skipped/manual coverage explicit.

## Test Plan

```bash
ci/gui_e2e_smoke.sh
GREENTIC_CHECK_E2E=1 ci/gui_e2e_functional.sh
GREENTIC_DESKTOP_REAL_DESKTOP=1 ci/gui_e2e_desktop_manual.sh
```

## Risks

- Making too much mandatory too early can block development. Start with smoke and mock functional suites, then promote stable suites to required status.

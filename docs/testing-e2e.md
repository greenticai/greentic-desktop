# End-To-End Testing

Greentic Desktop uses Playwright to validate the real GUI against a local
`greentic-desktop` process. Required suites use deterministic fixtures only:
no public websites, no live LLM keys, and no real desktop permissions.

## Suites

| Suite | Command | Purpose | Required |
| --- | --- | --- | --- |
| Smoke | `ci/gui_e2e_smoke.sh` | Starts the GUI, validates tokenized API access, and captures setup state. | Yes |
| Functional | `GREENTIC_CHECK_E2E=1 ci/gui_e2e_functional.sh` | Exercises setup, extensions, LLM mock planning, web automation, fake native/Java backends, recording/replay, runner updates, and MCP lifecycle. | Main/nightly |
| Real desktop | `GREENTIC_DESKTOP_REAL_DESKTOP=1 ci/gui_e2e_desktop_manual.sh` | Runs permission-sensitive desktop automation on a prepared host. | Manual release evidence |
| Real Java | `GREENTIC_DESKTOP_REAL_JAVA=1 ci/gui_e2e_desktop_manual.sh` | Runs accessibility-backed Java fixture checks on a prepared host. | Manual release evidence |
| Real LLM | `GREENTIC_DESKTOP_REAL_LLM=1 ci/gui_e2e_desktop_manual.sh` | Runs provider-backed checks with locally configured secrets. | Manual release evidence |

`ci/gui_e2e_release_report.sh` prints the release gate summary used by CI
artifacts. Manual suites should attach the Playwright report and test-results
directories from `frontend/automate-hub`.

## Local Setup

Install dependencies once:

```bash
node -v # must be 20.19+ or 22.12+
npm --prefix frontend/automate-hub ci
cd frontend/automate-hub
npx playwright install chromium
```

Then run the deterministic gates:

```bash
ci/gui_e2e_smoke.sh
GREENTIC_CHECK_E2E=1 ci/gui_e2e_functional.sh
```

`ci/local_check.sh` keeps GUI E2E opt-in for local development:

```bash
GREENTIC_CHECK_E2E_SMOKE=1 ci/local_check.sh
GREENTIC_CHECK_E2E=1 ci/local_check.sh
```

## CI Matrix

`GUI E2E Smoke` runs on Linux, macOS, and Windows for pull requests and pushes
to main/master. `GUI E2E Functional` runs on main/master, nightly, and manual
dispatch. `GUI E2E Real Desktop Manual` is dispatch-only and requires explicit
inputs for real desktop, Java, or LLM coverage.

## Prepared Host Notes

macOS real desktop tests require Screen Recording, Accessibility, and
Input Monitoring permissions for the launched binary or the terminal app used
to run it. Restart Greentic Desktop after granting permissions if macOS asks.

Windows real desktop tests require an interactive desktop session with UI
Automation access and the Calculator app installed.

Linux real desktop tests should use a known X11 session. Wayland automation is
permission-gated and may require portal approval.

Real LLM tests read provider configuration and API keys from the normal
Greentic settings/secrets path. Do not place live keys in required CI.

## Troubleshooting

Failures attach `greentic-desktop.log`, browser console output, API snapshots,
screenshots, traces, and video when available. The most useful local paths are:

- `frontend/automate-hub/playwright-report`
- `frontend/automate-hub/test-results`

If Chromium cannot start on macOS from a sandboxed runner, run the command from
an unsandboxed terminal session. If setup state is unexpected, remove the test
runtime home and rerun; Playwright creates an isolated home for every test.

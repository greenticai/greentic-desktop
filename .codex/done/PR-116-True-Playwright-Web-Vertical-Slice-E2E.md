# PR-116 - True Playwright Web Vertical Slice E2E

## Goal

Create one real end-to-end vertical slice that proves the product works through prompt/runner/package/MCP/replay/evidence against a real app.

## User Outcome

A user can describe a simple web automation, Greentic creates a runner, publishes it as MCP, an MCP client calls it, Playwright drives a local web app, and outputs/evidence are returned.

## Current Evidence

- Existing tests mainly assert structs, YAML, or JSON text.
- No test proves: prompt -> package -> MCP tools/call -> real browser automation -> output -> evidence.
- Playwright is the most practical first real adapter because a fixture web app can run in CI.

## Scope

1. Add a deterministic local fixture web app:
   - a single HTML/JS form served by a tiny test server.
   - fields: company name, email, operation/result, or invoice status.
   - predictable output element with stable selectors.
2. Add a real Playwright execution path for this fixture:
   - no model-only adapter.
   - no fake outputs.
   - fail if Playwright/browser cannot run.
3. Add an E2E harness that runs:
   - create/plan runner from prompt or load a checked-in generic runner.
   - save/package runner.
   - start MCP stdio server from PR-115.
   - use an MCP client fixture to call `tools/list`.
   - call `tools/call` with inputs.
   - assert browser-visible side effect happened.
   - assert output matches DOM result.
   - assert evidence bundle exists and references real run artifacts.
4. Make CI install only the required browser dependencies for this test.
5. Add failure logs:
   - browser console.
   - Playwright trace or screenshot.
   - MCP request/response transcript.

## File Targets

- `crates/greentic-desktop-web/src/lib.rs`
- `crates/greentic-desktop-mcp/src/lib.rs`
- `crates/greentic-desktop-test-harness/src/lib.rs`
- `tests/e2e/*` or `crates/greentic-desktop-test-harness/tests/*`
- `.github/workflows/*`
- `ci/local_check.sh`

## Out of Scope

- Native desktop app E2E.
- Full recorder E2E.
- Hosted browser grid.

## Acceptance Tests

1. CI starts the fixture app and proves it is reachable.
2. A runner fills real DOM fields and clicks a real submit button.
3. Output extraction reads the real DOM result.
4. MCP `tools/list` exposes the runner.
5. MCP `tools/call` returns the output.
6. Evidence bundle exists on disk and includes at least one real artifact reference.
7. If Playwright is missing, the test fails with an actionable setup message, not a fake pass.

## Done Means

This single test is the release gate for “Greentic can run one real automation through MCP.”

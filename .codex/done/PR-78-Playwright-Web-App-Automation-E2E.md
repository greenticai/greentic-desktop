# PR-78 - Playwright Web App Automation E2E

## Goal

Prove Greentic can create, run, test, record, replay, and publish a web-app automation end to end using Playwright against deterministic local web fixtures.

## Problem

Using public sites such as Google Calculator is brittle in CI because DOM, region, localization, bot detection, network, and cookie prompts change. We still need a real browser DOM workflow that behaves like a user-facing app.

## Test Apps

Build local fixture pages served by the Playwright test process:

1. **Calculator Web App**
   - route: `/calculator`
   - fields: `number_1`, `operation`, `number_2`
   - operations: plus, minus, multiply, divide
   - result element: `data-testid="result"`
   - expected: `1 + 1 = 2`
2. **Invoice Web App**
   - route: `/invoice`
   - input: `invoice_id`
   - button: `Lookup`
   - output: `total`
   - expected fixture: `INV-100 -> 42.50`
3. **Login-like Web App**
   - route: `/login`
   - inputs: username, password
   - output: session id
   - expected: password is never stored in runner YAML, logs, trace, or evidence

## Scope

1. Add `e2e/web-automation.spec.ts`.
2. Add fixture server helper using Playwright `webServer` or in-test static server.
3. Create a prompt runner for the web calculator:
   - prompt describes app URL, inputs, operation, output
   - generated runner has web adapter capabilities
   - runner input form asks for three fields
   - running with `1`, `1`, `plus` returns `2`
4. Record a web fixture interaction:
   - start recording as Browser task
   - perform fixture interaction
   - stop/normalise
   - assert normalized steps include `web.goto`, `web.fill`, `web.click`, `web.extract_text`
   - test recording output
   - save runner
5. Test secret redaction:
   - record login fixture
   - mark password as secret
   - final YAML uses secret reference
   - evidence/logs do not include raw password
6. Publish web runner as MCP tool:
   - save runner
   - publish
   - MCP page lists tool
   - call tool through GUI
   - output matches expected result

## Optional Public Smoke

Add a non-CI `@manual` test for Google Calculator only:

- search Google Calculator or use Google search query
- skip if cookie consent or bot detection appears
- never block release on this test

## Acceptance Criteria

- Local web calculator returns correct output through GUI runner Run.
- Web recording produces a reusable runner.
- Secret values are redacted end to end.
- Web runner can be published and called as MCP.
- Tests run without external network access.

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@web"
cargo test -p greentic-desktop-web -p greentic-desktop-recorder -p greentic-desktop-replay
```

## Risks

- A fake web fixture can miss real-browser edge cases. Keep fixtures realistic with labels, roles, dynamic output, validation errors, and delayed rendering.

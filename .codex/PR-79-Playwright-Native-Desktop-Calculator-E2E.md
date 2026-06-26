# PR-79 - Playwright Native Desktop Calculator E2E

## Goal

Verify native desktop automation works for the platform Calculator app where possible, while keeping CI deterministic through fake/native-contract tests and optional permissioned real-desktop tests.

## Problem

The product promise includes desktop apps, but real desktop automation requires OS-specific permissions, display sessions, app availability, and non-headless runners. Browser Playwright cannot directly drive OS windows, so tests must coordinate the GUI plus backend adapters and make capability gating explicit.

## Supported Targets

### macOS

- app: Calculator.app
- command: `open -a Calculator`
- permissions: Accessibility, Input Monitoring, Screen & System Audio Recording
- optional real test tag: `@desktop-real @macos`

### Windows

- app: Calculator
- command: `calc.exe`
- permissions: UI Automation/input policy as required
- optional real test tag: `@desktop-real @windows`

### Linux

- app: GNOME Calculator or KCalc if installed
- X11: possible under `xvfb-run` with fixture app preferred
- Wayland: expected blocked/portal flow unless a prepared session grants portal screenshot/input
- optional real test tags: `@desktop-real @linux-x11`, `@desktop-real @linux-wayland`

## Deterministic CI Fixture

Before real app tests, add a fake native calculator adapter:

- accepts open app, type text, click, read text actions
- simulates Calculator state
- returns result for plus/minus/multiply/divide
- writes evidence references
- exposes same adapter capabilities as native platform adapter

## Scope

1. Add `e2e/native-calculator.spec.ts`.
2. Add backend test mode to register fake native calculator adapter for CI.
3. Test prompt-to-runner:
   - use calculator prompt from PR-77
   - assert fields `number_1`, `number_2`, `operation`, `result`
   - save runner
   - run with `1`, `1`, `plus`
   - assert output `result = 2`
4. Test recording flow with fake native adapter:
   - choose Desktop app task
   - screen capture ready in fake mode
   - start recording
   - fake adapter emits app activation, input, click, output observed
   - normalise/save
   - test runner and assert result
5. Test permission gating:
   - macOS screen capture missing mode
   - desktop/remote targets show unavailable/warning
   - Open permission settings returns manual/opened message
   - Home and Recording agree about permission status
6. Add optional real Calculator tests:
   - require `GREENTIC_DESKTOP_REAL_DESKTOP=1`
   - preflight permissions
   - open Calculator
   - input `1 + 1`
   - observe output `2`
   - close Calculator

## Acceptance Criteria

- CI proves the desktop calculator flow through fake native adapter.
- Permission missing state blocks real capture honestly and consistently.
- Optional real macOS/Windows/Linux tests are documented and can be run locally.
- The real test skips with a clear reason if permissions or apps are missing.
- Real test never leaves Calculator open after completion.

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@desktop-fake"
GREENTIC_DESKTOP_REAL_DESKTOP=1 npm --prefix frontend/automate-hub run e2e -- --grep "@desktop-real"
cargo test -p greentic-desktop-macos -p greentic-desktop-windows -p greentic-desktop-linux
```

## Risks

- Real Calculator UI labels vary by OS version and locale. Use OS APIs where possible and skip with a readable diagnostic if the expected automation surface is unavailable.

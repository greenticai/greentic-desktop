# PR-76 - Playwright GUI Setup, Config, and Extension E2E

## Goal

Use Playwright to verify the Automate Hub setup, settings, permissions, extension install/update/remove, and LLM configuration flows work from the actual browser UI against the actual local GUI API.

## Problem

Recent bugs were visible only in the browser:

- setup buttons opened settings inconsistently
- token errors surfaced only after clicking UI buttons
- Home and Recording disagreed about screen capture status
- extension install needed permission approval but surfaced as a trust-policy block
- installed/removed extension state did not refresh reliably
- LLM provider list was incomplete

These are product-level regressions that Rust tests alone do not catch.

## Scope

1. Add `e2e/setup-config.spec.ts`.
2. Use PR-75 harness to launch the real GUI.
3. Test setup checklist rendering:
   - runtime home exists
   - browser extension installed/missing state
   - screen capture/accessibility/input permissions
   - MCP server configured
4. Test setup actions:
   - clicking setup buttons sends tokenized API requests
   - no `A valid GUI session token is required` error is shown
   - success/manual messages appear in red only for errors and neutral text for manual instructions
   - screen capture status on Home and Recording target screen comes from the same backend state
5. Test extension workflows:
   - recommended extensions load
   - search finds Playwright, Vision, Java, Terminal
   - install Playwright
   - install Vision and approve screen capture permission
   - verify installed list refreshes immediately
   - health/test an extension
   - disable/enable
   - remove with confirmation
   - verify recommended list shows Install again after removal
6. Test LLM settings:
   - provider list includes Local, OpenAI, Anthropic, Azure OpenAI, Gemini, Mistral, DeepSeek, OpenAI-compatible, NVIDIA NIM, Ollama
   - changing provider updates default model and endpoint
   - saving an API key does not reveal the secret back to the UI
   - clearing the API key removes it
   - test connection reports configured/missing-key status

## Fixtures

Run deterministic modes:

- `GREENTIC_DESKTOP_TEST_PERMISSIONS=all_ready`
- `GREENTIC_DESKTOP_TEST_PERMISSIONS=screen_capture_missing`
- `GREENTIC_DESKTOP_LLM_MOCK_DRAFT_JSON=<valid json>` for LLM test flows

If these env hooks do not exist yet, add them in the GUI API layer behind `cfg(test)` or `GREENTIC_DESKTOP_E2E=1`.

## Playwright Assertions

For every mutating action:

- wait for network response
- assert response `ok: true` or expected typed error
- assert no GUI token error toast
- assert UI state updates without page refresh
- assert `/api/v1/*` state matches visible UI

## Acceptance Criteria

- Setup checklist and recording screen never disagree about screen capture permission.
- All setup buttons perform visible, tokenized API calls.
- Extension install requiring screen capture asks for approval and succeeds after approval.
- Extension install/remove/update refreshes both installed and recommended views.
- LLM provider list matches `greentic-desktop-llm::supported_provider_profiles`.
- Secrets are write-only in the UI.

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@setup|@config|@extensions"
cargo test -p greentic-desktop-gui
```

## Risks

- Native OS settings cannot be asserted reliably in CI. Assert the backend response, visible instructions, and refreshed permission status, and leave real OS navigation to optional headed/manual tests.

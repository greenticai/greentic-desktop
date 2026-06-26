# PR-60 - Generic Web Adapter Semantics

Goal: remove remaining app-specific behavior from the Playwright web adapter model and align it with the shared generic workflow/output extraction model.

Problem
The web adapter still treats clicking a submit target as "Customer created" and emits `customer_id = CUST-1001`. This makes the web adapter less generic than the desktop adapters.

Design
Remove runtime side effects from `web.click`. The adapter should model generic browser behavior only:

- `web.goto`
- `web.fill`
- `web.select`
- `web.click`
- `web.wait_for_text`
- `web.extract_text`
- `web.extract_regex`
- `web.assert_visible`
- `web.assert_url`
- `web.screenshot`
- `web.download_file`

Output data should come from seeded fixture observations in tests or from real browser execution in production sidecar implementations, not from hard-coded customer creation logic.

Add generic web workflow support through the shared `DesktopWorkflow` compiler.

Acceptance criteria
No `Customer created`, `customer_id`, or CRM-specific web behavior remains in adapter execution.
Web tests seed visible text and identifiers as fixtures.
Web workflow tests use generic examples such as search, form submit, download, and extract text.
Replay output extraction works for web selectors, visible text, and regex.
Web adapter behavior is consistent with native, Java, terminal, and vision adapters.

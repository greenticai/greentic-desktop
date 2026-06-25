# PR-04 Playwright Web Adapter

## Goal

Support web-based applications through Playwright.

## Runtime Shape

The Rust host starts a Playwright sidecar, normally implemented in Node.js.

```text
greentic-desktop
  ↔ playwright-adapter-node
      ↔ browser
```

## Capabilities

```text
web.goto
web.click
web.fill
web.select
web.wait_for_text
web.extract_text
web.extract_regex
web.screenshot
web.assert_visible
web.assert_url
web.download_file
```

## Selector Strategy

Priority order:

1. `data-testid`
2. Accessible role/name
3. Label
4. Text
5. CSS selector
6. XPath
7. Visual fallback

Example:

```yaml
target:
  preferred:
    role: button
    name: Save
  fallback:
    text: Save
  css: "[data-testid='save-customer']"
```

## Recording

The adapter should record:

- URL
- DOM snapshot
- clicked element metadata
- stable selectors
- typed values with secret redaction
- screenshots
- network errors
- console errors

## Validation

Validation can include:

```yaml
success_criteria:
  - text_visible: "Customer created"
  - url_contains: "/customers/"
  - no_console_errors: true
  - output_exists: customer_id
```

## Acceptance Criteria

- Can open a URL.
- Can fill and submit a form.
- Can extract returned identifiers.
- Can produce stable selectors from a recorded human interaction.
- Can replay a recorded web runner on another compatible machine.

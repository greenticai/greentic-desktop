# Playwright Web Adapter

Use `greentic.desktop.playwright` for browser and web-application automation.

## Install

```bash
greentic-desktop extension install greentic.desktop.playwright
greentic-desktop extension verify greentic.desktop.playwright
greentic-desktop extension list
```

This is a sidecar extension. To inspect its launch metadata:

```bash
greentic-desktop extension sidecar greentic.desktop.playwright
```

The current manifest launches:

```text
node ./index.js
```

## When To Use It

Use this adapter for:

- internal web portals,
- SaaS applications,
- CRM or ticketing workflows,
- browser-based admin consoles,
- pages with stable selectors or accessibility labels,
- tasks that need text extraction or downloads.

Prefer this adapter over vision fallback when the page has usable selectors, labels, roles, or text.

## Engine And Protocol

Greentic keeps Playwright as the primary web engine because it provides reliable recording, cross-browser automation, downloads, screenshots, auto-waiting, and semantic locators in one maintained stack. The Rust adapter launches the Playwright sidecar over stdio and exchanges line-delimited typed JSON requests and responses with stable request ids.

The sidecar protocol covers observe, step execution, assertions, screenshots, downloads, and recorder events. A response id must match the request id; sidecar exits and invalid JSON are surfaced as structured adapter errors instead of being treated as generic replay failures.

`chromiumoxide`, `thirtyfour`, and `fantoccini` remain useful candidates for narrower backends:

- `chromiumoxide`: suitable for Chromium-only CDP automation, but weaker for cross-browser recording.
- `thirtyfour`: suitable when an environment already standardizes on Selenium/WebDriver.
- `fantoccini`: useful for lightweight WebDriver flows, but not a replacement for Playwright recorder coverage.

Do not replace Playwright for general web recording unless the replacement proves equivalent recording fidelity, download handling, locator robustness, and browser coverage.

## Capabilities

- `web.goto`
- `web.click`
- `web.fill`
- `web.select`
- `web.wait_for_text`
- `web.assert_visible`
- `web.assert_url`
- `web.extract_text`
- `web.extract_regex`
- `web.screenshot`
- `web.download_file`

## Runner Planning

Plan a web runner from a prompt:

```bash
greentic-desktop runner plan \
  --prompt "Open the CRM web app, create a customer from company name and email, and return the customer id" \
  --profile local-crm \
  --out ./runners/crm.create_customer.draft.yaml
```

The planner validates that installed adapters can provide capabilities such as `web.goto`, `web.fill`, `web.click`, and `web.extract_text` before writing the draft.

## Recording

Start a web recording session:

```bash
greentic-desktop record start \
  --name crm.create_customer \
  --profile local-crm \
  --adapter greentic.desktop.playwright \
  --out ./recordings/crm.create_customer \
  --redact text,password,email,token \
  --secret-fields password,api_key
```

Mark useful fields while recording:

```bash
greentic-desktop record mark-input company_name --session rec_123
greentic-desktop record mark-input email --session rec_123
greentic-desktop record mark-output customer_id --session rec_123
greentic-desktop record add-assertion "Customer created" --session rec_123
```

Stop and normalise:

```bash
greentic-desktop record stop --session rec_123

greentic-desktop record normalise \
  --recording ./recordings/crm.create_customer/raw \
  --out ./runners/crm.create_customer.draft.yaml
```

## Locator Guidance

Prefer stable web locators in this order:

- `data-testid` or app-owned test IDs,
- accessible role and name,
- form labels,
- stable text,
- CSS selectors,
- XPath only when there is no better stable locator,
- visual fallback as a last resort.

Avoid raw coordinates for browser workflows. They are fragile across screen sizes, zoom levels, and layout changes.

## Use As An MCP Tool

After review, package/sign/publish the runner and expose it as an MCP tool. Start the managed endpoint from Automate Hub **My Runners**.

Then call it from an MCP client using `tools/call`:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "crm.create_customer",
    "arguments": {
      "company_name": "Example Ltd",
      "email": "buyer@example.com"
    }
  }
}
```

## Permissions And Notes

The built-in manifest requests `network.localhost`. Production deployments should also control which web origins, credentials, and download locations a runner may use.

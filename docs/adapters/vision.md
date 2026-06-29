# Vision Screenshot Fallback Adapter

Use `greentic.desktop.vision` when structured automation is unavailable or unreliable.

## Install

```bash
greentic-desktop extension install greentic.desktop.vision
greentic-desktop extension verify greentic.desktop.vision
greentic-desktop extension list
```

This is a sidecar extension. To inspect its launch metadata:

```bash
greentic-desktop extension sidecar greentic.desktop.vision
```

The current manifest launches:

```text
greentic-vision-adapter
```

## When To Use It

Use this adapter for:

- apps without accessible UI metadata,
- remote desktops where only screenshots are available,
- visual fallback for another adapter,
- checking that a screen or dialog looks correct,
- finding text or buttons by screenshot,
- comparing against visual baselines.

Prefer structured adapters when possible. Vision is useful as a fallback, but it is more sensitive to theme, scaling, localization, and layout changes.

## Capabilities

- `vision.screenshot`
- `vision.find_text`
- `vision.find_button`
- `vision.click_region`
- `vision.compare_baseline`
- `vision.assert_visual`
- `vision.extract_text`

## Runner Planning

Plan a vision-backed runner:

```bash
greentic-desktop runner plan \
  --prompt "Use screenshots to find the Submit button, click it, and confirm that the Success message appears" \
  --profile visual-fallback \
  --out ./runners/vision.submit_success.draft.yaml
```

Be explicit about visible text, button labels, screen regions, and the visual assertion that proves success.

## Recording

Start a vision recording:

```bash
greentic-desktop record start \
  --name vision.submit_success \
  --profile visual-fallback \
  --adapter greentic.desktop.vision \
  --out ./recordings/vision.submit_success \
  --redact text,password,email,token \
  --secret-fields password
```

Add assertions and notes while recording:

```bash
greentic-desktop record add-assertion "Success" --session rec_123
greentic-desktop record note "Button appears in the lower-right panel" --session rec_123
```

Normalise:

```bash
greentic-desktop record stop --session rec_123

greentic-desktop record normalise \
  --recording ./recordings/vision.submit_success/raw \
  --out ./runners/vision.submit_success.draft.yaml
```

## Locator Guidance

Use vision locators carefully:

- crop to stable regions where possible,
- prefer visible text over raw coordinates,
- set confidence thresholds deliberately,
- keep baseline screenshots current,
- add structured assertions when another adapter can verify the result,
- avoid destructive actions that rely only on visual matches.

## Use As An MCP Tool

Start MCP after the runner is approved:

```bash
greentic-desktop mcp serve
```

Example MCP call:

```json
{
  "jsonrpc": "2.0",
  "id": 2,
  "method": "tools/call",
  "params": {
    "name": "vision.submit_success",
    "arguments": {}
  }
}
```

## Permissions And Notes

The built-in manifest requests `desktop.screenshot`. macOS, Linux Wayland, remote desktops, and managed workspaces may require explicit screenshot or screen-recording permission before this adapter can work.

Plain `vision.screenshot` uses the shared `xcap` backend and returns a real screenshot artifact or a concrete capture error. Text extraction, visual clicking, OCR, and remote viewport actions require a configured production backend; a screenshot match alone is not proof that a durable business side effect happened.

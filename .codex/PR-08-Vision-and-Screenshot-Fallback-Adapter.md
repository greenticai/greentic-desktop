# PR-08 Vision and Screenshot Fallback Adapter

## Goal

Provide fallback automation for inaccessible apps, Citrix/remote desktop sessions, unusual legacy apps and cases where DOM/UI Automation is not available.

## Capabilities

```text
vision.screenshot
vision.find_text
vision.find_button
vision.click_region
vision.compare_baseline
vision.assert_visual
vision.extract_text
```

## Use Cases

- Citrix-hosted apps
- Remote desktops
- Legacy apps without accessibility tree
- Java apps with poor metadata
- Image-based validation after patching
- Visual regression testing

## Design

Vision fallback should be used last, not first.

Preferred order:

```text
app-specific tool
  → adapter-specific structured locator
  → keyboard shortcuts
  → visual fallback
```

## Evidence

Every visual action must store:

- Screenshot before action
- Annotated target region
- Confidence score
- Screenshot after action

## Acceptance Criteria

- Can locate visible text on screen.
- Can click a button by visual/text recognition.
- Can compare current screen against baseline.
- Can explain why a visual assertion passed or failed.

# PR-06 Java Desktop Adapter

## Goal

Support Java desktop applications, including Swing and JavaFX where accessibility metadata is available.

## Technology Options

- Java Access Bridge
- Accessibility API
- Keyboard/mouse fallback
- Vision fallback

## Capabilities

```text
java.find_window
java.find_component
java.click_component
java.type_text
java.read_text
java.assert_visible
java.capture_tree
```

## Challenges

Java desktop apps often have inconsistent accessibility metadata. The adapter should support hybrid execution:

```text
accessibility tree if available
  → keyboard shortcuts
  → coordinate/vision fallback
```

## Recording

Capture:

- Java component tree
- Window titles
- Component names
- Role metadata
- Screenshots
- Keyboard shortcuts

## Acceptance Criteria

- Can inspect a Java desktop app when Java Access Bridge is enabled.
- Can record and replay simple form actions.
- Can fall back to keyboard and visual strategies.

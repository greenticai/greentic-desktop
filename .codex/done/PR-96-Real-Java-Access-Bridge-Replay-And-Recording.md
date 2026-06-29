# PR-96 - Real Java Access Bridge Replay and Recording

## Goal

Replace the Java in-memory adapter with real Java Access Bridge integration.

## User Outcome

For Swing/AWT/Java desktop apps, Greentic can find components, click/type/select/read text, and record user interactions through Java accessibility metadata.

## Current Evidence

- `JavaDesktopAdapter::execute` mutates `JavaState`.
- `JavaAccessBridgeRecordingBackend::start` emits a synthetic focused Java window event.
- It can return `"java fallback step accepted"` even when Access Bridge is disabled.

## Scope

1. Add Java Access Bridge sidecar or platform-specific bridge:
   - Windows JAB APIs.
   - Linux/macOS Java accessibility paths where available.
2. Implement:
   - `java.find_window`
   - `java.find_component`
   - `java.type_text`
   - `java.click_component`
   - `java.select`
   - `java.read_text`
   - `java.capture_tree`
   - `java.assert_text`
3. Remove product fallback that accepts Java steps without Access Bridge.
4. Add recording event source for component focus, value, invoke, and selection changes.
5. Persist component tree snapshots in evidence.
6. Make non-Java app prompts route away from Java unless the target process is Java or user selected Java.

## E2E Fixtures

1. Java Swing fixture app with fields, buttons, combo box, table, and save-to-file flow.

## Acceptance Tests

1. Without Access Bridge, Java adapter advertises no executable Java capabilities.
2. With Access Bridge, fixture runner types real values and reads real output.
3. Java recording produces component locators stable across replay.
4. Non-Java Word/document prompts do not select Java capabilities.
5. No Java step can pass through `"java fallback step accepted"` in production.


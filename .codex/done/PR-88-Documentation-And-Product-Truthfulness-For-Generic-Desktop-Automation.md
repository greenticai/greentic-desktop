# PR-88 - Documentation and Product Truthfulness for Generic Desktop Automation

## Goal

Update documentation, GUI copy, and runbooks so Greentic Desktop accurately describes what is supported, what requires setup, and how prompt/record/replay/MCP work for generic desktop automation.

## User Outcome

Users understand when they should see the desktop being driven, when a task runs headlessly/remotely, what permissions are needed, and why a prompt or recording may ask questions or block.

## Problem

The GUI currently looks complete even when core automation paths are fake, blocked, or incomplete. Documentation still contains CRM/calculator examples that can be mistaken for product capabilities.

## Scope

1. Rewrite getting-started around generic automation primitives:
   - open/attach
   - input
   - command/save
   - observe/extract
   - assert
   - MCP call
2. Replace default examples with generic resource/table/form examples.
3. Keep calculator/CRM only as optional fixtures, clearly labeled.
4. Document OS permission requirements:
   - macOS Accessibility/Input Monitoring/Screen Recording
   - Windows UI Automation/session restrictions
   - Linux X11/Wayland differences
5. Document visible vs headless execution.
6. Document failure modes and error codes.
7. Add release checklist item: no fake success paths in product runtime.

## Acceptance Tests

1. Docs explain the spreadsheet-style workflow as a generic app/resource automation, not an Excel-specific feature.
2. GUI copy does not promise recording/run support when required adapter permissions are unavailable.
3. Examples include expected inputs, outputs, and evidence.
4. Docs link setup requirements to the same readiness checks used by the app.


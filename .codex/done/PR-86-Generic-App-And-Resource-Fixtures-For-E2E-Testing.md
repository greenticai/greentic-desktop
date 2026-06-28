# PR-86 - Generic App and Resource Fixtures for E2E Testing

## Goal

Build reusable fixtures that test generic desktop automation without hard-coding product logic for calculator, Excel, CRM, or spreadsheets.

## User Outcome

CI fails when the app cannot actually prompt, record, replay, edit, and call a runner through MCP for representative desktop automation patterns.

## Problem

Many tests passed while runtime paths fabricated success. We need fixtures that force real execution and output extraction through adapters.

## Scope

1. Add a generic web fixture app:
   - form fields
   - table/grid
   - create/open resource simulation
   - append row
   - save indicator
   - output fields
2. Add a generic native fixture app where practical:
   - simple cross-platform window with text fields/table/save button
   - accessible labels/roles
   - no Excel dependency
3. Add a terminal fixture:
   - prompts for file/resource name and row fields
   - writes to `/tmp`
   - prints saved status/output
4. Add Playwright GUI tests for:
   - prompt-to-runner
   - run with user inputs
   - MCP call
   - edit existing runner
   - recording where supported
5. Add no-hard-code tests scanning product runtime for known demo shortcuts.

## Acceptance Tests

1. Prompt: create/open resource in `/tmp`, append name/email row, save.
2. Web fixture passes end-to-end.
3. Terminal fixture passes end-to-end.
4. Native fixture either passes on supported OS or reports a precise missing permission/adapter.
5. Tests fail if outputs are fabricated.
6. Tests fail if inputs are empty for prompts requiring user-provided values.


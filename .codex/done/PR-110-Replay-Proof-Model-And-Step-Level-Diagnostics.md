# PR-110 - Replay Proof Model and Step-Level Diagnostics

## Goal

Make replay results explain exactly which primitive and low-level step failed, with evidence and remediation.

## User Outcome

The test runner no longer shows only "step failed". It shows what failed, why, what was observed, and what to fix.

## Current Evidence

- The GUI currently reports `step failed` for the Word save flow.
- Users cannot tell whether the issue is app focus, locator, permissions, missing path, missing file, or unsupported primitive.

## Scope

1. Add replay trace hierarchy:
   - primitive id
   - primitive label
   - compiled adapter step id
   - adapter id
   - status
   - error code
   - message
   - evidence references
2. Add proof types:
   - file exists
   - file contains
   - UI text observed
   - screenshot captured
   - accessibility tree captured
   - dialog accepted
3. Update `replay_with_context` to return primitive trace data.
4. Update GUI test runner:
   - show failed primitive.
   - show failed compiled step.
   - show observed window/dialog text.
   - show exact missing permission/adapter/path.
5. Update MCP output:
   - include structured failure details.
   - include evidence bundle URI.

## Out of Scope

- New primitives.
- Adapter implementation.

## Acceptance Tests

1. A failed save flow reports `SaveResourceAs` as the failed primitive.
2. A failed file proof reports the exact missing path.
3. A failed locator reports the target query and observed accessible text count.
4. GUI displays step-level error details.
5. MCP callers receive the same structured error details.


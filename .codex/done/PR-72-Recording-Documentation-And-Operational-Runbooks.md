# PR-72 - Recording Documentation and Operational Runbooks

## Goal

Document exactly how real recording works for each target type, including what Greentic can and cannot record, required permissions, and troubleshooting steps.

## Problem

Users currently expect "Record" to capture arbitrary tabs/apps because the UI does not explain ownership and permission boundaries. Documentation must align with actual behavior after PR-63 through PR-71.

## Scope

1. Add user documentation for:
   - web recording
   - native desktop recording
   - Java recording
   - terminal/mainframe recording
   - remote desktop recording
2. Add developer documentation for:
   - adding a recording backend
   - raw event schema
   - normalizer contract
   - evidence/redaction rules
   - E2E fixture requirements
3. Add troubleshooting runbooks:
   - macOS permissions when launched from Terminal/VS Code/Cursor
   - Windows elevated app limitations
   - Linux X11 vs Wayland behavior
   - Java Access Bridge setup
   - terminal session ownership
   - browser controlled-context limitations
4. Add GUI help links or inline copy that points to the relevant runbook.

## Acceptance Criteria

- Docs state that Greentic-owned browser/terminal sessions are recorded first; arbitrary existing tabs/windows require later extension/attach support.
- Each target type has a "How to test locally" section.
- Each target type lists permissions and expected blocked states.
- Docs include a matrix matching PR-71's test matrix.
- No documentation claims recording works for a target unless an E2E test exists or the limitation is explicit.

## Done Means

Users and developers can understand how to run, verify, and troubleshoot real recording without guessing.


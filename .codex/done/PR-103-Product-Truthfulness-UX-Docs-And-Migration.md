# PR-103 - Product Truthfulness UX, Docs, and Migration

## Goal

Make the product and documentation accurately describe what is installed, healthy, executable, recordable, and blocked.

## User Outcome

Users can tell whether a workflow will run for real before saving it. Error messages explain what to install, approve, or configure.

## Current Evidence

- UI has shown installed adapters while run/test still used model-only adapters.
- Some docs imply broad automation capability that is not yet implemented.
- Setup state has sometimes appeared inconsistent after install/remove.

## Scope

1. Add adapter readiness dashboard:
   - installed
   - enabled
   - sidecar running
   - permissions granted
   - executable capabilities
   - recordable targets
2. Update Create/Test Runner screens:
   - show whether runner is executable with real adapters.
   - block test button when only model planning exists.
   - show exact missing adapter/permission/dependency.
3. Update runner cards:
   - Run disabled unless production-ready.
   - Test disabled unless production-ready.
   - MCP tool unavailable when runner is unavailable.
4. Update docs:
   - clear target support matrix.
   - setup per OS.
   - what runs visibly vs headless.
   - recording ownership limitations.
5. Add migration note for existing draft runners created with adapter-id step capabilities.
6. Add release checklist item: no production mock paths.

## Acceptance Tests

1. UI never shows a runner as runnable when `runner.real_adapter_missing` would occur.
2. Settings updates adapter readiness immediately after install/remove.
3. Docs support matrix matches runtime health output.
4. Existing draft runners with invalid capability fields are flagged with migration guidance.
5. Error banners include exact remediation for missing permissions/dependencies.


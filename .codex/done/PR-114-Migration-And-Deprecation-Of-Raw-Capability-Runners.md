# PR-114 - Migration and Deprecation of Raw Capability Runners

## Goal

Migrate existing raw capability runner drafts to primitive workflows and deprecate direct raw-step generation for new runners.

## User Outcome

Existing runners keep working where possible, but new prompt/recording flows use the more reliable primitive model.

## Current Evidence

- Existing drafts may contain `windows.*`, `macos.*`, or adapter-id capabilities directly.
- Regenerating drafts is currently required after planner fixes.

## Scope

1. Add runner manifest version:
   - legacy raw steps.
   - primitive workflow.
   - compiled adapter steps.
2. Add migration command/API:
   - load raw runner.
   - infer primitives.
   - show confidence and open questions.
   - write migrated draft.
3. Add GUI migration banner for raw runners.
4. Block new prompt-generated raw runners unless explicitly imported as legacy.
5. Keep MCP execution compatibility for legacy runners until migration is complete.
6. Add docs:
   - why primitives exist.
   - migration limitations.
   - how to inspect generated workflow.

## Out of Scope

- Perfect migration for every legacy click sequence.
- Deleting legacy support.

## Acceptance Tests

1. A raw macOS document runner migrates to primitives with confidence metadata.
2. A raw Windows runner on macOS is flagged as foreign-platform legacy.
3. New prompt-generated runners include primitive workflow manifests.
4. Legacy runners still fail closed with clear diagnostics when unsupported.
5. GUI exposes migrate/review/save flow.


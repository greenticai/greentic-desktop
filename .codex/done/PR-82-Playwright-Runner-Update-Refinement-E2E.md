# PR-82 - Playwright Runner Update and Refinement E2E

## Goal

Verify that saved runners can be edited and updated through prompts, input/output changes persist, diffs are meaningful, tests rerun, and failed-runner refinement produces real changes instead of placeholders.

## Problem

Runner update UX is high-risk because it can appear to update a runner while only changing UI state. We need E2E tests that inspect saved files/API state and then rerun the runner.

## Scope

1. Add `e2e/runner-update.spec.ts`.
2. Seed a runner by creating the calculator prompt runner.
3. Test metadata edit:
   - click edit icon
   - current task/prompt is preloaded
   - change runner name
   - save
   - runner list and MCP tool name update consistently
4. Test prompt-based update:
   - prompt: `Also support multiply and return result_text`
   - fake LLM returns structured patch
   - GUI shows diff with changed inputs/outputs/steps
   - apply patch
   - runner file changes
   - run/test uses updated fields
5. Test manual input/output wizard persistence:
   - add input `rounding_mode`
   - add output `result_text`
   - save
   - reload page
   - fields are still present
   - run form asks for new input
6. Test failed-runner refinement:
   - run with missing required input
   - failure shown with evidence
   - click refine/fix
   - provide correction
   - preview diff
   - apply
   - rerun succeeds
7. Test discard:
   - generate update diff
   - discard
   - runner file unchanged

## Acceptance Criteria

- Edit opens the existing task, not a blank prompt.
- Input/output additions persist across reloads.
- Prompt updates are schema-backed and reviewable before apply.
- Apply changes actual runner content.
- Discard leaves runner unchanged.
- Run/test after update uses the new schema.
- Evidence and errors remain available after failure and refinement.

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@runner-update|@refinement"
cargo test -p greentic-desktop-refinement -p greentic-desktop-gui -p greentic-desktop-test-harness llm_golden_update_fixture_patches_runner_schema
```

## Risks

- Diffs can become flaky if they are text-only. Prefer structured patch assertions and only snapshot text diffs lightly.

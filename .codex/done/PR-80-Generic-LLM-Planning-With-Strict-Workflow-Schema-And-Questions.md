# PR-80 - Generic LLM Planning with Strict Workflow Schema and Questions

## Goal

Change prompt-to-runner planning from flattened step JSON to strict `RunnerDefinition` / `DesktopWorkflow` JSON, with repair loops, open questions, capability routing, and no calculator/CRM defaults.

## User Outcome

For a prompt like “create/open a spreadsheet in `/tmp`, add a row with name/email, save,” Greentic should infer generic inputs/actions/outputs when clear, and ask targeted questions when required details are missing, such as which app to use or whether CSV/LibreOffice/Excel should be preferred.

## Current Evidence

- `greentic-desktop-llm` valid example is calculator-specific.
- `HeuristicLlmClient` infers `number_1`, `number_2`, and `operation` from calculator-shaped prompts.
- `greentic-desktop-planner` still has `infer_inputs`, `infer_outputs`, and `infer_steps` with narrow keyword logic.
- `plan_prompt_with_llm` parses `RunnerDraftDocument`, not `RunnerDefinition`.

## Problem

LLM planning is not currently a generic desktop automation planner. It asks for a flattened runner draft and validates only capabilities/fields. It does not require a target, an open strategy, typed actions, output extractors, or resource assertions. This makes non-calculator desktop workflows under-specified and un-runnable.

## Scope

1. Replace the LLM request schema with `RunnerDefinition` / `DesktopWorkflow` schema.
2. Include adapter capabilities and platform information in the prompt:
   - open app/file
   - find element/component
   - type/click/keyboard shortcut
   - read/extract
   - file/resource operations
   - save/assert
3. Add a strict JSON repair loop:
   - validate JSON
   - validate schema
   - validate capabilities
   - validate extractors
   - send concise repair prompt on failure
4. Add `open_questions` with machine-readable question ids.
5. Make the heuristic fallback generic:
   - extract nouns after “ask for”, “provided”, “with”
   - infer `spreadsheet_name`, `name`, `email` for the spreadsheet prompt
   - infer required app/file question when target is ambiguous
   - never default to calculator or CRM
6. Add policy to reject vague runners that lack:
   - target/open strategy
   - at least one executable action
   - required input schema for user-provided values
   - output extractor or assertion for requested outputs/saves

## Acceptance Tests

1. Spreadsheet prompt returns typed inputs `spreadsheet_name`, `name`, `email`.
2. Spreadsheet prompt produces generic actions: open/create resource, input row values, save, assert saved.
3. If the app choice is ambiguous, planner returns an open question instead of silently choosing Excel.
4. Calculator prompt can still plan, but only through generic app/input/read actions, with no product hard-code.
5. LLM response missing action ids or extractors is repaired or rejected with a clear diagnostic.


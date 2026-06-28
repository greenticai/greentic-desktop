# PR-85 - Input, Output Extractor UX, and Secrets for Generic Runners

## Goal

Make the GUI expose typed inputs, secrets, output extractors, assertions, and open questions for prompt-created and recorded runners.

## User Outcome

Before saving or running, users can see exactly which values Greentic will ask for, which secrets are needed, what outputs will be returned, and what evidence/extractors prove the run worked.

## Current Evidence

- Create wizard displays only string lists of inputs/outputs.
- Manual input/output edits are not tied to typed schema/extractors.
- Run form asks for inputs but does not show secrets or extractor requirements.
- LLM provider secrets are separate from runner secrets and need consistent handling.

## Problem

Generic automation cannot be trustworthy if outputs are just names. The system must know how to extract `saved_path`, `status`, `row_count`, or any other output from a target observation.

## Scope

1. Add typed field editor:
   - name
   - type
   - required
   - default
   - enum choices
   - validation
2. Add secret editor backed by `greentic-secrets-lib`.
3. Add output extractor editor:
   - target text
   - visible text rule
   - regex
   - terminal field
   - JSON path
   - file/resource assertion
4. Add open-question answer UI that updates the draft.
5. Make Run/Test forms generated from typed input schemas.
6. Show output values and extraction proof after run.

## Acceptance Tests

1. Spreadsheet-style draft shows `spreadsheet_name`, `name`, `email` as editable typed inputs.
2. Adding an input/output manually persists into the typed manifest and is used by replay.
3. Missing required input blocks Run/Test.
4. Missing required secret prompts to save it through secrets storage.
5. Output extractor failures are visible and actionable.


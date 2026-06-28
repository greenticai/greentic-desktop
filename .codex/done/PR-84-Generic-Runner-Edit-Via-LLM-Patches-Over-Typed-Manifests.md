# PR-84 - Generic Runner Edit via LLM Patches over Typed Manifests

## Goal

Replace keyword-based edit patching with schema-validated LLM patches over typed runner manifests, with open questions, test-before-apply, version history, and capability validation.

## User Outcome

A user can edit an existing workflow by saying “also add a phone column to the row” or “also return the saved file path,” and Greentic updates that runner instead of creating a new one or only appending vague YAML text.

## Current Evidence

- `infer_runner_patch_plan` only knows keywords such as precision, discount, invoice, alias, expression, clipboard, subtract, multiply, divide.
- Edit test path fabricates outputs.
- Patch operations are not applied to typed workflow actions/extractors because the current persisted model is flat YAML.

## Problem

Updating a generic desktop automation requires semantic changes: inputs, actions, locators, output extractors, assertions, risk, and capability requirements. Keyword patching cannot handle “add another column to the spreadsheet” generically.

## Scope

1. Define a typed runner patch schema:
   - add/update/remove input
   - add/update/remove secret
   - add/update/remove action
   - add/update/remove output extractor
   - add/update/remove assertion
   - update target/open strategy
   - update risk/policy
2. Request patches from the configured LLM with strict schema.
3. Validate and repair LLM patch JSON.
4. Apply patches to `RunnerDefinition`.
5. Recompile and validate capability routes.
6. Require test success before apply unless user explicitly saves a draft.
7. Preserve version history and rollback.

## Acceptance Tests

1. Editing the spreadsheet-style runner with “add a phone column” adds input `phone` and updates the append-record action.
2. Editing with “also return the saved file path” adds a typed output extractor/assertion.
3. Vague edit prompts return open questions, not arbitrary changes.
4. Invalid patches are rejected with readable errors.
5. The source runner id remains unchanged after apply.


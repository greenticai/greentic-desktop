# PR-106 - LLM Planning Generates Primitives, Not Raw Adapter Steps

## Goal

Update prompt-based runner creation so the LLM produces typed desktop primitives with strict schema validation instead of raw adapter capability steps.

## User Outcome

A prompt like "open Word, create a document, add text, save it" becomes a workflow with `NewResource`, `TypeText`, `SaveResourceAs`, and `AssertResourceExists`, not a fragile list of arbitrary clicks and fields.

## Current Evidence

- The LLM can return `windows.activate_app`, `windows.click`, and other invalid/foreign capabilities.
- The planner has to normalize low-level output after the fact.
- Complex user tasks lose semantic intent too early.

## Scope

1. Update `LlmRequestEnvelope`:
   - include primitive JSON schema.
   - require primitives as the primary output.
   - forbid raw adapter capabilities except inside an explicit legacy escape hatch.
2. Add strict parser for primitive plans.
3. Add repair loop:
   - schema mismatch
   - unsupported primitive
   - missing inputs
   - missing outputs
   - ambiguous app/resource
4. Add planner questions:
   - which app should open the resource?
   - where should the file be saved?
   - what output should be returned?
   - should an existing file be overwritten?
5. Derive input fields from primitive parameters:
   - `document_name`
   - `save_location`
   - `text_content`
6. Derive output fields from proof primitives:
   - `file_path`
   - `saved_status`
7. Persist generated primitive workflow in runner manifest.
8. Compile to adapter steps only after schema validation and capability routing.

## Out of Scope

- Recorder changes.
- Full GUI redesign.

## Acceptance Tests

1. The Word prompt generates primitives:
   - `OpenApp`
   - `NewResource`
   - `Focus(document_body)`
   - `TypeText`
   - `SaveResourceAs`
   - `AssertResourceExists`
2. The generated input list contains `document_name`, `save_location`, and `text_content`.
3. The generated output list contains a proof-backed saved file output.
4. Invalid LLM JSON enters repair once before failing.
5. Raw foreign OS capabilities from the LLM are rejected before runner save.


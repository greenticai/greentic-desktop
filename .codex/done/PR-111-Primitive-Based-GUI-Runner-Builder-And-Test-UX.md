# PR-111 - Primitive-Based GUI Runner Builder and Test UX

## Goal

Update the prompt wizard, runner editor, and test runner UI to display and edit primitive workflows.

## User Outcome

Users can see that a runner will "create document" and "save as file" before testing, rather than a confusing list of low-level adapter steps.

## Current Evidence

- Users enter sample values and get opaque failures.
- The GUI hides the workflow semantics and therefore cannot guide corrections.

## Scope

1. Add primitive workflow preview:
   - app
   - resource
   - inputs
   - outputs
   - primitives
   - proof assertions
2. Add editable primitive cards:
   - input binding
   - target selector
   - path template
   - overwrite policy
   - output mapping
3. Add test runner improvements:
   - input form from primitive parameters.
   - run button.
   - step-by-step trace.
   - output/proof panel.
   - evidence links.
4. Add repair UX:
   - "this failed at Save As".
   - ask missing question.
   - allow user correction via prompt.
5. Prevent saving a runner that has unsupported primitives unless it is saved as draft with open questions.

## Out of Scope

- New adapter functionality.
- LLM provider settings.

## Acceptance Tests

1. Word prompt preview shows `NewResource`, `TypeText`, `SaveResourceAs`, `AssertResourceExists`.
2. Test runner shows primitive trace and outputs.
3. Unsupported primitive disables final save or requires explicit draft state.
4. User can edit `save_location` binding before running test.
5. Error display includes exact primitive and evidence links.


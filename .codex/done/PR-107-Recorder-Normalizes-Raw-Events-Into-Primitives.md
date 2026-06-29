# PR-107 - Recorder Normalizes Raw Events Into Primitives

## Goal

Normalize recorded low-level UI events into the same typed workflow primitives used by prompt-generated runners.

## User Outcome

Recording a user creating and saving a document produces an editable, portable workflow, not a brittle coordinate/click transcript.

## Current Evidence

- Recording currently captures events but does not consistently infer durable intent.
- The product still struggles to turn recorded desktop actions into reusable generic automation.

## Scope

1. Add recorder normalization pipeline:
   - raw events
   - grouped interactions
   - inferred primitives
   - compiled adapter steps
2. Infer primitives from event sequences:
   - app activation -> `OpenApp` / `ActivateApp`
   - menu File > New -> `NewResource`
   - document body typing -> `TypeText`
   - File > Save As dialog -> `SaveResourceAs`
   - file chooser path entry -> `ResourcePath`
   - final file existence -> `AssertResourceExists`
3. Add target classification:
   - document body
   - text field
   - dialog field
   - menu item
   - button
   - table/cell
4. Capture input markers and output markers as primitive parameters.
5. Preserve raw events as evidence, but not as the main workflow.
6. Add confidence and open questions when normalization is ambiguous.

## Out of Scope

- New native event tap implementations.
- LLM planning.

## Acceptance Tests

1. A synthetic recording of "new document, type text, save as file" normalizes into primitives.
2. A web recording normalizes into web primitives.
3. A terminal recording normalizes into terminal primitives.
4. Ambiguous save dialog interactions produce an open question instead of a false success.
5. Raw coordinate clicks are retained only as fallback evidence, not primary workflow steps.


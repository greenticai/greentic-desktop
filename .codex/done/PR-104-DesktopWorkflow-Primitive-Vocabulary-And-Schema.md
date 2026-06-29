# PR-104 - DesktopWorkflow Primitive Vocabulary and Schema

## Goal

Introduce a first-class, typed desktop workflow primitive layer above raw adapter capabilities.

## User Outcome

Users can describe tasks like "create a document, add text, save it, and return the file" without the planner pretending those inputs are visible form fields in the app.

## Current Evidence

- Word document prompts compile to low-level `find_element`, `type_text`, `click_element`, and `read_text` steps.
- The runner cannot represent generic intents like `new document`, `save as path`, or `assert file exists`.
- Tests fail with opaque `step failed` because the semantic task is lost before replay starts.

## Scope

1. Extend `greentic-desktop-workflow` with a typed primitive enum:
   - `OpenApp { app_ref }`
   - `ActivateApp { app_ref }`
   - `WaitForWindow { match, timeout }`
   - `FocusWindow { match }`
   - `NewResource { resource_type }`
   - `OpenResource { path_or_uri }`
   - `SaveResource`
   - `SaveResourceAs { path }`
   - `AssertResourceExists { path }`
   - `CloseResource { save_policy }`
   - `ExportResource { format, path }`
   - `Focus { target }`
   - `TypeText { text }`
   - `SetField { target, value }`
   - `ChooseMenu { path }`
   - `PressKey { key_combo }`
   - `Click { target }`
   - `WaitUntil { condition, timeout }`
   - `ExtractText { target }`
   - `ExtractArtifact { path_or_uri }`
2. Add supporting typed models:
   - `AppRef`
   - `ResourceType`
   - `ResourcePath`
   - `TargetQuery`
   - `MenuPath`
   - `WorkflowCondition`
   - `SavePolicy`
3. Preserve existing `RunnerStep` for compiled adapter actions.
4. Add schema export for primitives in `greentic-desktop-runner-schema`.
5. Add serde round-trip tests for the primitive schema.
6. Add migration helpers that can wrap existing raw steps as `Primitive::AdapterStep` only for legacy runner compatibility.

## Out of Scope

- Adapter-specific execution.
- LLM prompt generation.
- Recording normalization.

## Acceptance Tests

1. A workflow can represent:
   - open app
   - create document
   - type body text
   - save as path
   - assert file exists
2. The JSON schema includes every primitive and required fields.
3. Existing raw-step runner packages still deserialize.
4. No primitive success can be represented without either a compiled adapter step or a proof-producing assertion.


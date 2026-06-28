# PR-79 - Use DesktopWorkflow RunnerDefinition as the Canonical Runner Model

## Goal

Stop saving and editing thin YAML that only contains `inputs`, `outputs`, and flat `steps`. Make `RunnerDefinition` with `DesktopWorkflow` the canonical persisted model for prompt-created and recorded runners.

## User Outcome

A user can describe generic work such as “open an app, create/open a file, enter values, save, read confirmation,” and the runner stores the intent, target, inputs, actions, output extractors, assertions, risk, and compiled steps without losing semantics.

## Current Evidence

- `crates/greentic-desktop-workflow` defines `DesktopWorkflow`, `WorkflowTarget`, `WorkflowAction`, `WorkflowOutputExtractor`, and compile logic.
- `crates/greentic-desktop-runner-schema` defines `RunnerDefinition`.
- The GUI planner currently persists `draft.render_yaml()` from `RunnerPackage`, which loses target/open/output extractor semantics.
- `runner_files`, `runner_summary_json`, `yaml_list`, and edit logic read ad-hoc YAML lists rather than a typed schema.

## Problem

The spreadsheet prompt needs semantics that flat steps cannot represent safely:

- target: local desktop app or file-backed task
- open: app by name, executable, bundle id, or associated file
- file path input: `/tmp/{{inputs.spreadsheet_name}}`
- actions: create-if-missing, open-if-existing, append row, save
- outputs: saved path/status/observed row count
- assertions: file exists, row visible/saved

The current YAML shape cannot preserve this reliably, so prompting and editing degrade into vague steps.

## Scope

1. Define a versioned persisted runner manifest:
   - `schema_version`
   - `runner_definition`
   - compiled steps
   - source prompt or recording metadata
   - capability requirements
2. Add serde load/save helpers for runner manifests.
3. Make GUI list/detail/read/edit paths use typed runner manifests first.
4. Provide migration for existing flat `RunnerPackage` YAML into a minimal `RunnerDefinition`.
5. Make `render_yaml` output typed manifests, not only flat lists.
6. Include output extractors in persisted files.

## Model Extensions Needed

Add generic workflow concepts if missing:

- `WorkflowOpenTarget::FileOrApp`
- `WorkflowActionKind::CreateResource`
- `WorkflowActionKind::Save`
- `WorkflowActionKind::AppendRecord`
- `WorkflowValueType::Path`
- file/resource assertions such as exists/modified/saved

These concepts must remain generic and map to app, web, terminal, or file-backed adapters.

## Acceptance Tests

1. A spreadsheet-style prompt produces a `RunnerDefinition` with:
   - inputs: spreadsheet name, name, email
   - target/open metadata
   - append-record/save actions
   - output extractor or assertion for saved result
2. Loading and saving the runner round-trips without dropping workflow actions or extractors.
3. Existing flat YAML runners still load through migration.
4. Editing an existing runner modifies the same typed manifest, not a lossy YAML projection.


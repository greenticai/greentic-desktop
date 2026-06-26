# PR-58 - Serde Runner Schema and JSON Schema Export

Goal: replace ad-hoc JSON parsing with typed serialization and exported schemas for planner responses, runner packages, MCP inputs, MCP outputs, and workflow definitions.

Problem
The runner draft parser manually scans strings for JSON fields. That is fragile for LLM output, MCP interoperability, GUI forms, and package validation.

Design
Add `serde` derives for runner-schema types:

- `RunnerDefinition`
- `DesktopWorkflow`
- `WorkflowTarget`
- `WorkflowInput`
- `WorkflowAction`
- `WorkflowOutput`
- `WorkflowAssertion`
- `RunnerDraftDocument`
- `McpInputSchema`
- `McpOutputSchema`

Use `serde_json` for parsing/rendering. Use a JSON Schema generation crate or a local schema renderer to export stable schemas.

Compatibility
Keep the current parser as a fallback only if necessary during migration, but all new planner and MCP code should use typed `serde_json`.

Validation
Validation should produce structured diagnostics:

- missing required field
- invalid enum value
- unsupported capability
- unsupported target technology
- missing input schema
- missing output extractor
- unsafe action missing approval

Acceptance criteria
LLM planner output is parsed through `serde_json`.
Runner schema JSON Schema can be exported for prompts and MCP clients.
Invalid planner output returns precise diagnostics.
Existing runner package tests migrate to typed parsing.
No new code manually scans JSON strings.

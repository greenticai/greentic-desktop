# PR-61 - Generic MCP Published Runner Fixtures

Goal: ensure MCP published-runner examples and tests are technology-agnostic and schema-driven.

Problem
MCP tests and example tools currently center on `crm.create_customer` with email/password/customer_id. That is acceptable as one example but not as the default product shape.

Design
Replace single CRM example helper with generic fixture builders:

- `published_runner_tool_for_web_form`
- `published_runner_tool_for_native_app`
- `published_runner_tool_for_java_form`
- `published_runner_tool_for_terminal_lookup`
- `published_runner_tool_for_vision_extraction`

MCP descriptors should expose schemas generated from runner definitions:

- concrete input schema
- concrete output schema
- risk metadata
- approval requirements
- evidence policy
- required adapters

MCP call behavior
`tools/call` should:

- validate typed inputs and secrets
- enforce policy
- call replay
- return typed outputs
- include evidence URI
- return structured failure codes

Acceptance criteria
CRM is only one optional fixture, not the default MCP example.
MCP tests cover at least five runner technologies.
Tool descriptors include real input and output schemas, not only static schema refs.
MCP call results reflect replay output extraction.
Forwarded tool names remain stable for arbitrary runner IDs.

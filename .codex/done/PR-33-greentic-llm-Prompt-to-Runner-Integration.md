# PR-33 greentic-llm Prompt-to-Runner Integration

## Goal

Wire `greentic-llm` into the Greentic Desktop prompt-to-runner flow so natural language instructions can be converted into validated runner drafts through the same LLM abstraction used elsewhere in Greentic.

This PR makes PR-11 implementation-ready instead of leaving the planner as a conceptual component.

## Background

PR-11 defines the prompt-to-runner planner and says the planner should produce JSON/YAML that is validated before use.

The missing piece is the concrete LLM integration layer:

```text
user prompt
  → planning context builder
  → greentic-llm request
  → structured runner draft
  → schema validation
  → policy/risk validation
  → saved draft runner package
```

## Scope

### Crates / modules

```text
crates/
  greentic-desktop-planner/
  greentic-desktop-llm/
  greentic-desktop-runner-schema/
  greentic-desktop-policy/
```

The `greentic-desktop-llm` crate should be a thin adapter around `greentic-llm`, not a second LLM framework.

## CLI Commands

```bash
gtc desktop runner plan \
  --prompt "Create a runner that opens the CRM and creates a customer" \
  --profile local-crm \
  --out ./runners/crm.create_customer.draft.yaml

gtc desktop runner plan \
  --prompt-file ./prompt.md \
  --context ./desktop-context.json \
  --dry-run
```

## greentic-llm Contract

The planner should call `greentic-llm` using a structured request envelope.

```json
{
  "task": "desktop.prompt_to_runner",
  "model_policy": {
    "temperature": 0.1,
    "response_format": "json_schema",
    "max_retries": 2
  },
  "context": {
    "available_adapters": [],
    "available_mcp_tools": [],
    "session_profiles": [],
    "existing_runners": [],
    "ltm_examples": [],
    "security_policy": {},
    "desktop_observation": {}
  },
  "user_prompt": "Create a runner that opens the CRM and creates a customer"
}
```

## Structured Output Schema

`greentic-llm` must return a draft that conforms to the runner schema.

```json
{
  "runner_id": "crm.create_customer",
  "version": "0.1.0-draft",
  "summary": "Create a new CRM customer and return the customer id.",
  "risk_level": "medium",
  "required_capabilities": [
    "web.goto",
    "web.fill",
    "web.click",
    "web.extract_text"
  ],
  "inputs": {
    "company_name": { "type": "string", "required": true },
    "email": { "type": "string", "required": true }
  },
  "outputs": {
    "customer_id": { "type": "string" }
  },
  "steps": [],
  "assertions": [],
  "open_questions": []
}
```

## Planner Flow

```text
1. Build planning context from runtime state.
2. Ask greentic-llm for a structured runner draft.
3. Validate against the runner JSON schema.
4. Validate required capabilities against installed adapters.
5. Run security and policy checks.
6. Save the draft runner package.
7. Return actionable diagnostics if the draft is invalid.
```

## Context Builder

The planner context should include:

- Installed adapter capabilities.
- Active platform information.
- Current desktop observation if available.
- Session bootstrap profiles.
- Existing runner packages.
- Available MCP tools.
- LTM examples of similar successful runners.
- Security policy constraints.
- User-provided prompt and optional examples.

## Validation Rules

The planner must not trust raw LLM output.

Validation should include:

- JSON schema validation.
- Runner package schema validation.
- Required capability validation.
- Secret placeholder validation.
- Risk classification validation.
- Policy validation.
- Static step validation before any execution.

## Error Handling

Invalid LLM output should produce structured diagnostics.

```text
planner.invalid_json
planner.schema_mismatch
planner.unsupported_capability
planner.missing_required_input
planner.policy_denied
planner.needs_clarification
```

## Safety

The LLM may create draft runner packages but must not execute high-risk desktop actions directly.

High-risk actions require explicit approval before replay or recording-assisted execution.

Examples:

- Submitting forms.
- Sending emails.
- Deleting records.
- Making payments.
- Updating production systems.

## Tests

Add tests for:

- Mock `greentic-llm` returning a valid runner draft.
- Invalid JSON response.
- Schema-invalid response.
- Unsupported capability response.
- Policy-denied response.
- Prompt with missing required details.
- Prompt that requires an open question instead of hallucinating.

## Acceptance Criteria

- `gtc desktop runner plan --prompt ...` creates a draft runner package.
- Planner uses `greentic-llm` through the shared Greentic LLM abstraction.
- Planner output is validated before being saved.
- Unsupported adapters/capabilities fail before execution.
- Invalid LLM output produces useful diagnostics.
- The planner can run in dry-run mode without writing files.
- The planner does not execute high-risk actions without approval.

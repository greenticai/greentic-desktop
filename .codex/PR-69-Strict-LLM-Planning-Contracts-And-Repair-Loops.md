# PR-69 - Strict LLM Planning Contracts and Repair Loops

## Goal

Make LLM-backed prompt planning reliable by enforcing strict JSON schemas, provider-native structured output where available, validation, deterministic repair loops, and actionable diagnostics.

## Problem

The current first-draft planner calls the configured LLM once, parses the response as a runner draft, and fails if the JSON or schema is invalid. The request envelope says `json_schema`, but provider calls do not pass provider-native JSON schema controls, and there is no repair pass when a model returns malformed JSON, unsupported capabilities, invalid risk, missing inputs, or weak outputs.

Concrete gaps:

- `plan_prompt_with_llm` is single-shot.
- `LlmRequestEnvelope::render_json` is manually formatted JSON.
- `ConfiguredGuiLlmClient` sends ordinary chat prompts without `response_format` or tool/schema constraints.
- `runner_draft_json_schema()` is incomplete and not wired into provider requests.
- Invalid output immediately becomes a user-visible error instead of a model repair attempt.
- There is no persisted planning trace showing attempts, validation errors, schema version, or model/provider.

## User Outcome

When a user describes an automation, Greentic either produces a valid runner draft or asks clear questions. Bad model formatting should be repaired automatically. Unsupported capabilities, missing details, and policy violations should be surfaced as structured questions or blocked requirements, not vague JSON errors.

## Design

Introduce a strict planning contract:

```rust
pub struct LlmStructuredRequest {
    pub task: LlmTask,
    pub schema_name: String,
    pub schema_version: String,
    pub json_schema: serde_json::Value,
    pub system_instruction: String,
    pub user_payload: serde_json::Value,
    pub repair_context: Option<RepairContext>,
}

pub struct LlmStructuredResponse<T> {
    pub value: T,
    pub raw_content: String,
    pub attempts: Vec<LlmAttempt>,
}
```

Planning should run:

```text
build context
  -> call LLM with strict schema
  -> parse
  -> schema validate
  -> capability validate
  -> policy validate
  -> compile workflow
  -> if validation fails and retry budget remains, repair with validation diagnostics
  -> return draft or questions/blockers
```

## Provider Support

Use provider-native structured output where available:

- OpenAI-compatible: `response_format: { type: "json_schema", json_schema: ... }`.
- Azure OpenAI: same shape where supported by deployment; fall back to strict JSON instruction if not.
- Anthropic: tool-use style schema or JSON-only prompt with repair loop.
- Gemini: `response_schema` / structured generation where available; fallback to repair loop.
- Mistral: JSON response format/tool schema where available.
- Ollama/local: JSON-only prompt plus repair loop.

Provider implementation should expose capability flags:

```rust
pub enum StructuredOutputMode {
    NativeJsonSchema,
    NativeTool,
    PromptOnlyWithRepair,
}
```

## Schema Work

Use serde types as the source of truth:

- `RunnerDraftDocument`
- `RunnerDefinition`
- `DesktopWorkflow`
- `PlannerClarificationResponse`
- `PlannerRepairResponse`

Generate JSON schemas with `schemars` or an equivalent maintained crate. Avoid hand-maintained schema strings except as a temporary compatibility layer.

The planner schema must include:

- runner ID/name/description/intent
- target technologies
- inputs, secrets, validation, defaults
- outputs and extractors
- workflow actions
- assertions
- required capabilities
- risk and approval policy
- open questions
- assumptions
- confidence per step
- unsupported requirements

## Repair Loop

On failure, call the same model with:

- original prompt
- previous raw model output
- exact parse/schema/capability/policy/compiler diagnostics
- target schema
- instruction to return a full corrected JSON object

Repair loop limits:

- default max attempts: 3 total
- no temperature increase
- fail closed on policy-denied actions
- never repair by dropping user-required outputs without adding an open question

## Planning Trace

Persist `planning_trace.json` under the draft directory:

- provider/model
- schema name/version/hash
- request payload excluding secrets
- raw attempts
- validation diagnostics
- repair prompts
- final status
- open questions/blockers

Do not persist API keys or secret values.

## Acceptance Criteria

- First-draft planning uses strict schema controls for OpenAI-compatible providers.
- Invalid JSON is repaired automatically when possible.
- Schema mismatch is repaired automatically when possible.
- Unsupported capabilities become open questions or blocked requirements.
- Policy-denied actions remain blocked and are not repaired away silently.
- Planning trace is saved with all attempts and diagnostics.
- Unit tests prove invalid JSON, schema mismatch, unsupported capability, and compile errors trigger repair attempts.

## Test Plan

- Static LLM client test: attempt 1 invalid JSON, attempt 2 valid JSON.
- Static LLM client test: schema-invalid draft repaired to valid draft.
- Capability test: model selects unavailable adapter and repair switches to an installed capability or emits question.
- Policy test: high-risk denied action remains blocked.
- Provider request snapshot tests for OpenAI-compatible `response_format`.
- GUI API test verifies trace files are written and errors include user-actionable messages.

## Risks

- Provider structured output APIs differ. Keep provider mapping isolated in `greentic-desktop-llm`.
- Strict schemas can be too large. Start with concise draft schema and include detailed workflow schema only when needed.
- Repair loops can mask weak prompts. Keep assumptions and open questions visible in the UI.


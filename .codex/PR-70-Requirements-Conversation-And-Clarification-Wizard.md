# PR-70 - Requirements Conversation and Clarification Wizard

## Goal

Add a requirements-gathering loop so prompt-based runner creation asks the user clear questions before generating or saving an under-specified runner.

## Problem

The current prompt wizard sends the initial prompt directly to planning, then shows guessed inputs/outputs and steps. Open questions can exist in the draft, but they are not first-class UI state and do not drive a conversation. Missing application, credentials, output expectations, required inputs, destructive actions, or unsupported technologies can become weak drafts instead of explicit customer questions.

## User Outcome

The user describes a task. Greentic identifies missing requirements, asks concise questions, stores the answers, updates the requirements, and only then generates a runner draft with visible assumptions.

## Design

Introduce a `RunnerRequirements` document:

```rust
pub struct RunnerRequirements {
    pub task: String,
    pub target_app: Option<String>,
    pub target_technology: Option<TargetTechnology>,
    pub inputs: Vec<RequirementField>,
    pub secrets: Vec<RequirementField>,
    pub outputs: Vec<RequirementField>,
    pub constraints: Vec<String>,
    pub examples: Vec<RequirementExample>,
    pub assumptions: Vec<String>,
    pub open_questions: Vec<PlannerQuestion>,
    pub answered_questions: Vec<PlannerAnswer>,
}
```

The LLM should be able to return either:

- `needs_clarification` with questions
- `ready_to_plan` with requirements
- `blocked` with missing capability/setup/policy reasons

## Question Types

Support structured questions:

- free text
- single choice
- multiple choice
- input field definition
- output definition
- secret selection
- risk/approval confirmation
- app/technology selection

Each question should include:

- ID
- prompt text
- why it matters
- default/recommended answer if safe
- validation requirements
- whether it blocks planning

## GUI Plan

Update `/create?mode=prompt`:

1. User enters initial task.
2. Backend creates `requirements.json`.
3. If questions exist, show one requirements screen, not a generic error.
4. User answers questions.
5. Backend patches requirements and reruns the requirements analyzer.
6. Only proceed to draft generation when status is `ready_to_plan` or user explicitly accepts assumptions.

The inputs/outputs editor should edit requirements, not only patch generated YAML. Draft generation should use the updated requirements as source of truth.

## Backend API

Add:

```http
POST /api/v1/planner/requirements
GET /api/v1/planner/requirements/{id}
PATCH /api/v1/planner/requirements/{id}
POST /api/v1/planner/requirements/{id}/answer
POST /api/v1/planner/requirements/{id}/draft
```

Keep draft endpoints, but route new prompt creation through requirements first.

## LLM Prompting

Use a strict schema for `PlannerRequirementsResponse`. The model must not invent values when a blocking requirement is missing. It should ask questions for:

- unknown app/system URL
- unknown login/service account
- unspecified output
- ambiguous input names/types
- destructive/high-risk operation
- technology not supported by installed adapters
- whether visible/manual approval is acceptable

## Persistence

Store in each draft directory:

- `requirements.json`
- `requirements_history.jsonl`
- `answers.json`
- `assumptions.json`

These files become part of the audit trail and later refinement context.

## Acceptance Criteria

- Empty or vague prompt returns structured questions, not a failed draft.
- User answers are persisted and included in subsequent LLM calls.
- Inputs/outputs added in the wizard update requirements before draft generation.
- Draft generation uses requirements and answers, not only the original prompt.
- Saved runner includes summary of assumptions and required approvals.

## Test Plan

- Vague prompt test: asks for target app and output.
- Login prompt test: asks credential/service-account question.
- Calculator prompt test: identifies number inputs, operation enum, result output without extra questions.
- High-risk delete/payment prompt test: asks approval/risk confirmation.
- GUI API lifecycle test for requirements create, answer, draft.
- Frontend mocked-flow test for question rendering and answer persistence.

## Risks

- Too many questions slow users down. Mark non-blocking questions as assumptions with an explicit accept path.
- The model may ask redundant questions. Deduplicate by stable question ID and requirement field.


# PR-71 - LLM Planner Orchestrator and Capability-Aware Draft Generation

## Goal

Upgrade prompt-to-runner creation from a single model call into a planner orchestrator that routes by capabilities, gathers context, generates a generic `DesktopWorkflow`, validates it, and compiles it to a runner.

## Problem

The codebase has a capability router and a simple LLM draft path, but they are not deeply integrated. The model can emit required capabilities directly, yet there is no orchestrated sequence that chooses technology, fetches relevant adapter capabilities, uses installed runner examples, asks questions, generates a workflow, compiles it, and repairs failures.

## User Outcome

A first prompt-based runner is generated with the right technology and capabilities:

- web when DOM automation is best
- native accessibility when appropriate
- Java Access Bridge for Java apps
- terminal/mainframe for terminal flows
- vision only when semantic adapters are unavailable
- existing runner composition when a suitable runner already exists

## Planner Approach

Implement a multi-stage planner:

```text
requirements
  -> classify technology and risk
  -> route capabilities
  -> retrieve context/examples
  -> generate DesktopWorkflow
  -> compile to runner schema
  -> validate policy/capabilities
  -> repair or ask questions
  -> persist draft
```

## Planner Stages

### 1. Classify

LLM returns:

- target technologies ranked with confidence
- required adapters/capabilities
- existing runner/tool candidates
- risk level
- missing context questions

### 2. Retrieve

Backend supplies:

- installed adapter capabilities
- MCP tools/runners
- application metadata
- observed desktop state
- recent successful runner examples
- policy constraints
- user requirements and answers

### 3. Generate Workflow

LLM emits `DesktopWorkflow`, not low-level compiled steps. Workflow should use semantic actions:

- open/attach
- observe
- find
- input
- click
- key
- wait
- extract
- assert
- close

### 4. Compile

Use `greentic-desktop-workflow::compile_workflow` and `RunnerDefinition::from_workflow`. The model does not write final YAML by hand.

### 5. Validate and Repair

Run:

- JSON schema validation
- workflow compile validation
- capability validation
- policy validation
- input/output schema validation
- output extractor validation

Feed exact diagnostics into PR-69 repair loop.

## API Changes

`POST /api/v1/planner/drafts` should accept:

- prompt
- optional requirements ID
- optional target runner ID for updates
- optional mode: `create | update | compose`
- optional context paths/evidence refs

Response should include:

- draft ID
- requirements ID
- planner trace ID
- route
- confidence
- questions
- assumptions
- warnings
- input/output schemas
- workflow preview
- YAML preview

## Acceptance Criteria

- Planner emits and validates `DesktopWorkflow` before saving runner YAML.
- Existing runner/tool candidates are used when they match the task.
- The selected technology and capability route are visible in the draft response.
- Compile errors trigger repair attempts.
- Vision fallback is used only when semantic adapters are missing or explicitly requested.
- The generated runner has typed inputs/outputs and output extractors.

## Test Plan

- Calculator prompt routes to native app workflow when native adapter is available.
- Web CRM prompt routes to web workflow with role/label selectors.
- Mainframe prompt routes to terminal workflow.
- Existing runner prompt composes or suggests existing runner instead of duplicating.
- Missing adapter prompt returns blocked/question state.
- Compile-failure fake LLM response is repaired.

## Risks

- Multi-stage planning increases latency. Cache context and keep stages lightweight.
- Too much context can degrade model output. Use concise, ranked context summaries and include only relevant examples.


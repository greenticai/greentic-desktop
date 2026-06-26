# PR-77 - Playwright Prompt-to-Runner and LLM E2E

## Goal

Verify that creating a runner from a prompt works end to end through the GUI, including LLM provider configuration, strict JSON schema handling, input/output derivation, open questions, repair loops, draft persistence, testing, saving, and runner list visibility.

## Problem

The calculator prompt regression showed that the wizard can display empty inputs/outputs even when the planner has enough information. We need browser-level tests that prove the prompt wizard produces usable runner definitions, not just JSON responses.

## Primary Test Prompt

Use this exact prompt as a regression fixture:

```text
open the calculator app. Take three inputs: two numbers and one operation (plus, minus, divide or multiply) and make the calculator do the operation and return the result
```

Expected wizard fields:

- inputs: `number_1`, `number_2`, `operation`
- outputs: `result`
- no blocking open questions
- required adapter is native desktop on the current OS, or vision fallback if native adapter is not available

## Scope

1. Add `e2e/prompt-runner.spec.ts`.
2. Add deterministic mock LLM server or response hook.
3. Test local heuristic provider:
   - open Create
   - choose "Start with a prompt"
   - enter calculator prompt
   - generate draft
   - assert inputs/outputs are populated
   - add one extra input and output in wizard
   - persist changes
   - save runner
   - runner appears on Runners page with updated input/output fields
4. Test configured remote provider path:
   - set provider to OpenAI-compatible with local fake endpoint
   - fake endpoint returns invalid JSON first
   - repair loop request is made
   - second response returns valid strict JSON
   - trace file is created and visible through `/api/v1/planner/traces/{traceId}`
5. Test open question path:
   - vague prompt: `automate login`
   - wizard shows questions instead of silently creating a broken runner
   - answering required fields lets generation continue
6. Test schema/policy failure path:
   - fake LLM returns unsupported capability
   - GUI shows actionable error
   - no runner is saved

## Additional Prompts

Add golden prompt fixtures for:

- Web form: `Open the local invoice web app, enter invoice_id, submit, and return total`
- Terminal: `Connect to the local terminal fixture, enter account_id, and return balance`
- Java app: `Open the Java Swing fixture, enter customer name, click Save, and return confirmation id`
- Runner update: `Add operation as an input and return result as an output`

## Acceptance Criteria

- The calculator prompt never produces empty input/output arrays.
- Wizard-added inputs and outputs persist to saved runner files and runner detail API.
- Remote LLM path uses configured settings, not the local heuristic.
- Invalid LLM JSON triggers repair and traceability.
- Open questions block unsafe draft creation.
- Saved runner can be tested from the Runner page after creation.

## Test Plan

```bash
npm --prefix frontend/automate-hub run e2e -- --grep "@llm-mock|@prompt"
cargo test -p greentic-desktop-llm -p greentic-desktop-planner -p greentic-desktop-gui
```

## Risks

- Real LLM calls are nondeterministic and need secrets. CI must use a fake OpenAI-compatible endpoint. Add an optional `@manual @llm-real` suite for real provider smoke tests.

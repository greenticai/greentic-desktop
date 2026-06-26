# PR-73 - LLM Evaluation Fixtures and Golden Prompt Tests

## Goal

Add deterministic tests and evaluation fixtures that prove prompt-based creation and update flows are reliable across common automation scenarios.

## Problem

LLM behavior is probabilistic and provider-dependent. Without golden prompt fixtures, schema repair tests, and replay-oriented acceptance tests, prompt generation and refinement can regress while still compiling.

## User Outcome

Greentic can evolve its planner prompts and schemas with confidence. Common workflows keep producing valid, runnable, typed runners.

## Fixture Set

Create prompt fixtures for:

- Calculator: number 1, operation enum, number 2, result output.
- Web CRM create customer: company/email inputs, customer ID output.
- Supplier portal download: date input, downloaded file output.
- Mainframe lookup: account number input, status output.
- Native desktop report export: date range inputs, file output.
- Java Swing form update: form inputs and confirmation output.
- Existing runner composition: call one runner then another.
- High-risk payment/delete flow: approval questions and policy block.

Update fixtures for:

- add input
- add output
- update locator
- change target app/technology
- add wait/assertion
- make field secret
- remove obsolete step

## Evaluation Harness

Add a test harness that can run with:

- static model responses for deterministic CI
- recorded provider responses for regression
- live provider calls only when explicitly enabled by env var

Suggested commands:

```bash
cargo test -p greentic-desktop-planner llm_golden_prompts
cargo test -p greentic-desktop-refinement llm_update_golden_prompts
GREENTIC_DESKTOP_LLM_EVAL_LIVE=1 cargo test -p greentic-desktop-test-harness -- --ignored llm_live_eval
```

## Metrics

For each fixture, track:

- valid JSON on first attempt
- repairs needed
- schema validity
- compile validity
- capability validity
- policy validity
- required questions produced
- expected inputs/outputs present
- expected target technology selected
- replay/test pass with fake adapters

## Golden Files

Store under `crates/greentic-desktop-test-harness/fixtures/llm/`:

- `create/*.prompt.txt`
- `create/*.expected.json`
- `update/*.prompt.txt`
- `update/*.current.runner.json`
- `update/*.expected_patch.json`
- `provider-recordings/*.jsonl`

Provider recordings must not contain secrets.

## CI Integration

Add fast deterministic checks to `ci/local_check.sh`:

- schema generation test
- planner repair loop tests
- create fixture tests with static responses
- update fixture tests with static responses
- MCP schema verification after apply

Live provider tests remain ignored/manual.

## Acceptance Criteria

- Golden create fixtures validate the whole path from prompt to compiled runner.
- Golden update fixtures validate prompt to patch to candidate runner.
- Repair loop tests cover malformed JSON and schema-invalid output.
- CI does not require real API keys.
- Manual live eval can run against configured Settings/secret provider without persisting secrets.

## Risks

- Golden files can become stale when schemas improve. Version fixtures by schema version and include a migration note.
- Live provider tests can be expensive or flaky. Keep them opt-in and summarize rather than blocking ordinary CI.


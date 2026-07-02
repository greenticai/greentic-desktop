# PR-143 - Live Validation Recorder And Prompting Smoke Tests

## Goal

Use the live validation harness to prove that prompted runner creation, runner editing, and recording produce executable workflows.

## User Outcome

The core product loop can be tested end to end: describe or record a task, save the runner, run it, and verify the real app side effects.

## Current Evidence

- Prompted runner generation has produced invalid or incomplete runner definitions.
- Recording flows captured events but did not reliably produce executable workflows.
- The UI looked complete while real desktop automation still failed.

## Scope

1. Add live prompting smoke tests:
   - generate a web runner from a prompt.
   - generate a desktop runner from a prompt when the required app exists.
   - validate generated inputs/outputs are populated.
   - run the generated runner through `desktop validate`.
2. Add live runner edit smoke tests:
   - load an existing runner.
   - prompt to add a field/output/assertion.
   - validate the same runner id is updated.
   - run validation after edit.
3. Add recording smoke tests:
   - web recording against a local fixture page.
   - terminal recording against a PTY fixture.
   - desktop recording only when live capture permissions and supported app are available.
4. Add LLM gating:
   - if no configured provider/api key exists, mark LLM live tests as `missing_secret`, not passed.
   - optionally use deterministic fixture model only for non-live schema tests.
5. Store generated runner manifests and evidence in `target/greentic-live-validation/`.

## File Targets

- `crates/greentic-desktop-test-harness/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-cli/src/lib.rs`
- `docs/live-validation.md`

## Out of Scope

- Guaranteeing every arbitrary prompt works.
- Using mocks to claim live validation passed.

## Acceptance Tests

1. Prompting smoke test fails with `missing_secret` if the selected LLM provider has no API key.
2. Prompting smoke test fails if generated runner inputs/outputs are empty for a prompt that clearly requires them.
3. Runner edit smoke test preserves runner id and validates the edited runner.
4. Web recording smoke test records real browser actions and replays them against a local fixture.
5. Desktop recording smoke test is skipped with explicit setup reason when permissions or app are missing.

## Done Means

The live harness validates the actual product loop, not only hand-written YAML examples.

# PR-120 - LLM HTTP Client Provider Adapters and Structured Retry

## Goal

Replace `curl` subprocess calls with a real HTTP client and provider-specific request/response adapters.

## User Outcome

LLM planning is reliable, does not expose API keys in process arguments, supports DeepSeek and other listed providers consistently, and repairs invalid structured output with clear diagnostics.

## Current Evidence

- `crates/greentic-desktop-llm/src/lib.rs` shells out to `curl`.
- API keys can appear in process listings.
- No timeout/retry/backoff.
- Provider list includes providers that are not actually wired.
- Live LLM schema drift caused basic enum/field failures.

## Scope

1. Add async or blocking HTTP client:
   - preferred: `reqwest` with rustls.
   - configured timeout.
   - no subprocess argv secrets.
2. Provider adapters:
   - OpenAI.
   - DeepSeek.
   - Mistral.
   - Ollama.
   - mark unsupported providers as disabled until implemented.
3. Secret resolution:
   - use greentic secret store/env.
   - never log raw keys.
4. Structured output:
   - send JSON schema or JSON mode where provider supports it.
   - parse with typed schema.
   - repair loop with original validation error.
   - cap retries.
5. Live-provider test suite:
   - ignored by default.
   - `DEEPSEEK_API_KEY` live test for Word prompt.
   - `OPENAI_API_KEY` optional equivalent.
6. UI:
   - settings only labels providers as available if adapter exists.

## File Targets

- `crates/greentic-desktop-llm/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-planner/src/lib.rs`
- `Cargo.toml`
- `.github/workflows/*` optional scheduled live-provider workflow.

## Out of Scope

- Streaming UI.
- Fine-tuning or prompt analytics.

## Acceptance Tests

1. Unit test proves API key is placed in header, not command args.
2. DeepSeek mock server receives expected request shape.
3. Invalid enum response triggers repair prompt and then succeeds.
4. Exhausted repair attempts returns validation details.
5. Ignored live DeepSeek test can be run with `DEEPSEEK_API_KEY`.
6. Provider list does not claim unsupported live mode.

## Done Means

Prompt planning no longer depends on `curl` and live provider behaviour is part of the test strategy.

# PR-90 - Fail Closed Production Replay and Remove Mock Adapters

## Goal

Make it impossible for the shipped GUI, MCP server, or CLI runner paths to pass by using in-memory or capability-only adapters.

## User Outcome

When a user clicks **Run Test**, **Run**, or invokes an MCP tool, Greentic either performs real automation through a production adapter or returns a clear setup/error message. It must never say "Test passed" because a mock accepted a step.

## Current Evidence

- `crates/greentic-desktop-replay/src/lib.rs` has `CapabilityOnlyAdapter`, created by `AdapterRegistry::from_capabilities`.
- `crates/greentic-desktop-adapter/src/lib.rs` has `StaticAdapter`.
- GUI execution currently builds an in-process registry from model adapters.
- Output extraction previously accepted non-evidence step messages.

## Problem

The runtime contract is currently ambiguous: capability metadata can become executable adapters. This causes false success and hides missing production implementations.

## Scope

1. Move `CapabilityOnlyAdapter` behind `#[cfg(test)]` or delete it from non-test builds.
2. Move `StaticAdapter` behind `#[cfg(test)]` or a `test-fixtures` feature that is disabled by default.
3. Remove or gate `AdapterRegistry::from_capabilities` from production builds.
4. Add a production `ReplayAdapterRegistry` constructor that only accepts real adapter handles.
5. Make GUI, MCP, and CLI execution use the production registry.
6. Introduce typed error `runner.real_adapter_missing` when a runner cannot be executed by real backends.
7. Remove output extraction from generic step messages.
8. Validate path-like output evidence before reporting success.
9. Add a repo-wide test that fails if `CapabilityOnlyAdapter`, `StaticAdapter`, or fake recording backends are reachable from non-test product code.

## Acceptance Tests

1. A runner with `macos.activate_app` fails with `runner.real_adapter_missing` if no real macOS execution backend is configured.
2. A runner that outputs `/tmp/missing.docx` fails with `runner.output_extraction_failed` or `runner.execution_failed`, not `passed`.
3. `cargo test -p greentic-desktop-replay` still supports test fixture adapters under `#[cfg(test)]`.
4. `cargo test -p greentic-desktop-gui production_replay_gate_blocks_model_only_adapters` proves model-only replay is blocked.
5. `rg "CapabilityOnlyAdapter|StaticAdapter|fake backend heartbeat" crates --glob '!**/tests/**'` has no production call path except explicit test/fixture modules.

## Non-Goals

- This PR does not implement native OS automation. It prevents false success while later PRs add real backends.


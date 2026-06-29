# PR-136 - Real Signing Packaging And Schema Validation Foundation

## Goal

Complete the non-adapter production foundation: real signatures, gtpack packaging, and strict JSON schema validation through maintained crates.

## User Outcome

Runners and extension packages can be verified, packed, and validated without drift between Rust structs, LLM prompts, and JSON schemas.

## Current Evidence

- `sha2` is now used for registry digests, but signing should use real Ed25519 keys.
- Runner packaging must use `greentic-pack --answers answers.json`.
- LLM/schema validation needs `schemars` + `jsonschema`, not ad-hoc validation.

## Scope

1. Use `ed25519-dalek` for signing and verification:
   - runner manifests.
   - extension manifests where applicable.
   - registry records.
2. Keep `sha2` for package and artifact digests.
3. Ensure all `.gtpack` creation goes through `greentic-pack --answers answers.json`.
4. Add `schemars` derives for externally consumed schema structs.
5. Add `jsonschema` validation at boundaries:
   - LLM response.
   - runner import.
   - MCP tool schema generation.
   - recorder finalization.
6. Remove remaining `sha256:abc123` or fake signature literals from production code and fixtures that imply real signing.

## File Targets

- `crates/greentic-desktop-registry/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-desktop-runner-schema/src/lib.rs`
- `crates/greentic-desktop-cli/src/lib.rs`
- `crates/greentic-desktop-mcp/src/lib.rs`
- `docs/release.md`

## Out of Scope

- Enterprise KMS/HSM integration.
- Changing the public runner workflow model.

## Acceptance Tests

1. Ed25519 signed manifests verify and tampered manifests fail.
2. CLI runner pack invokes `greentic-pack --answers answers.json` and never writes a fake container itself.
3. LLM response schema validation uses generated schema from Rust structs.
4. Imported runner packages fail if JSON schema validation fails.
5. CI fails on new fake signature/digest literals in production code.

## Done Means

Packaging, signatures, and schemas are maintained by production-grade libraries and tooling, not local placeholders.

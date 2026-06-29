# PR-118 - Real Signing and SHA-256 Integrity

## Goal

Replace fake FNV/checksum “signatures” with real cryptographic package integrity and signatures.

## User Outcome

Published runners and packages cannot be silently modified without verification failure.

## Current Evidence

- `registry/src/lib.rs` uses FNV-1a as a deterministic hash and calls it signature-like.
- Some package checksums are placeholder strings such as `sha256:abc123`.

## Scope

1. Add SHA-256 hashing:
   - use `sha2`.
   - hash canonical package bytes and manifest fields.
2. Add Ed25519 signatures:
   - use `ed25519-dalek` or equivalent maintained crate.
   - sign manifest digest.
   - verify with public key.
3. Key handling:
   - local dev key generation command.
   - public key trust store under runtime home.
   - never commit private keys.
4. Replace existing fake signature fields:
   - keep migration support for legacy unsigned draft runners.
   - published/distributed packages require real signatures.
5. Enforce verification:
   - load/install rejects invalid signatures.
   - MCP publish hides untrusted packages.
6. Add docs for local dev signing and production trust.

## File Targets

- `crates/greentic-desktop-registry/src/lib.rs`
- `crates/greentic-desktop-security/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-desktop-extension/src/lib.rs`
- `Cargo.toml`

## Out of Scope

- Hardware key support.
- Enterprise key rotation service.

## Acceptance Tests

1. Signed package verifies with matching public key.
2. Modified runner content fails verification.
3. Modified manifest fails verification.
4. Wrong public key fails verification.
5. Legacy unsigned draft runner loads only when draft policy explicitly allows it.
6. Published runner without real signature is rejected.

## Done Means

No code path refers to FNV or placeholder checksum values as a production signature.

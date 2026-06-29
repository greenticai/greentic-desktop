# PR-117 - Use greentic-pack for .gtpack Build Verify Load

## Goal

Use `greentic-pack` as the single `.gtpack` implementation and integrate Greentic Desktop runner export/import with it.

## User Outcome

Runners can be exported, imported, verified, and distributed as real `.gtpack` files without Greentic Desktop hand-rolling another package/container format.

## Current Evidence

- `.gtpack` is central in product language but Greentic Desktop currently treats runner packages as structs/YAML.
- Package code currently manipulates structs and YAML/JSON, not a portable signed artifact.
- `greentic-pack` already owns package creation/verification semantics and must be the source of truth.

## Scope

1. Treat `greentic-pack` as the only `.gtpack` builder/verifier:
   - do not implement a separate archive writer/reader in this repo.
   - do not add independent `zip`/`tar` package layout code here.
   - call the `greentic-pack` CLI or, if available, depend on its library API.
2. Define the Greentic Desktop -> `greentic-pack --answers answers.json` contract:
   - Greentic Desktop generates a temporary `answers.json`.
   - `answers.json` contains the runner manifest, runner definition path, input/output schema paths, asset paths, evidence policy, signing metadata, and output `.gtpack` path in the shape expected by `greentic-pack`.
   - `answers.json` is written to a temp directory with safe permissions.
   - no interactive `greentic-pack` prompts are used in automation.
3. Add build command as an adapter over `greentic-pack --answers answers.json`:
   - `greentic-desktop runner pack <runner-id> --out file.gtpack`
   - internally writes `answers.json`.
   - runs `greentic-pack --answers <temp>/answers.json`.
   - fails if `greentic-pack` is missing, incompatible, or returns validation errors.
   - prints the exact `greentic-pack` diagnostic.
4. Add verify/load commands as adapters over `greentic-pack`:
   - `greentic-desktop runner verify-pack file.gtpack`
   - `greentic-desktop runner install-pack file.gtpack`
5. Integrate with registry/import paths:
   - verify through `greentic-pack` before install.
   - load only the verified manifest/runner payload produced by `greentic-pack`.
6. Preserve compatibility:
   - legacy YAML runners can still load locally.
   - distribution requires `.gtpack`.
7. Add version compatibility:
   - detect `greentic-pack --version`.
   - document minimum supported version.
   - fail closed on unsupported pack versions.

## File Targets

- `crates/greentic-desktop-registry/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-desktop-cli/src/lib.rs`
- `crates/greentic-desktop-runner-schema/src/lib.rs`
- optional small adapter module/crate for invoking `greentic-pack`, but not for archive implementation.
- `docs/*`

## Out of Scope

- Remote package registry.
- UI package upload flow.
- Reimplementing the `.gtpack` container format.
- Choosing compression/archive internals.

## Acceptance Tests

1. With a fake `greentic-pack` on PATH, `runner pack` invokes `greentic-pack --answers <answers.json>`.
2. The generated `answers.json` contains the runner manifest, runner definition, schemas, assets, and requested output path.
2. With real `greentic-pack` available, build a `.gtpack` from a runner with one asset.
3. Verify package through `greentic-pack`, not local archive parsing.
3. Load the package into a clean runtime home.
4. Corrupt one package file and verify fails through `greentic-pack`.
5. A malicious package/path traversal fixture is rejected by `greentic-pack` verification before install.
6. Build output is deterministic for the same inputs according to `greentic-pack` verification/hash output.
7. Missing or unsupported `greentic-pack` produces an actionable error.

## Done Means

Greentic Desktop produces and consumes `.gtpack` only through `greentic-pack`; this repo does not own a competing package format.

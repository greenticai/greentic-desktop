# PR-146 - GUI Runner Import API And Distributor Resolution

## Goal

Add GUI API endpoints that import runner YAML from uploaded content or a distributor-resolved source URI using the existing runner import logic.

## User Outcome

The frontend can import runner YAML safely from local upload, `oci://`, `store://`, and `repo://` sources with the same validation guarantees as the CLI.

## Current Evidence

- `greentic-desktop-cli` already exposes `--import` and `runner import`.
- `crates/greentic-desktop-gui/src/lib.rs` already contains `import_runner_yaml_file` and distributor source resolution paths used by CLI helpers.
- The GUI API currently exposes runner list/detail/action endpoints, but no direct runner import endpoint.

## Scope

1. Add `POST /api/v1/runners/import`:
   - accepts JSON body variants:
     - `{ "kind": "yaml", "filename": "...", "yaml": "..." }`
     - `{ "kind": "source", "source": "oci://..." }`
   - returns imported runner metadata:
     - runner id.
     - runner name.
     - path.
     - source URI, if any.
     - resolved URI/cache path, if any.
2. For YAML uploads:
   - parse with the same YAML-to-runner path as CLI import.
   - reject manifests that do not produce a typed runner package.
   - persist to runtime runner home using the existing safe filename logic.
3. For source imports:
   - accept only `oci://`, `store://`, `repo://`, and `file://`.
   - use `greentic-distributor-client` through `DesktopRuntime::resolve_runner_source`.
   - reject unresolved or missing cache artifacts with actionable errors.
4. Duplicate runner handling:
   - define explicit default behavior.
   - recommended default: fail with `runner.duplicate` unless `replace: true`.
   - if `replace: true`, preserve previous version history where possible.
5. Error model:
   - `runner.import_invalid_yaml`.
   - `runner.import_unsupported_source`.
   - `runner.import_resolution_failed`.
   - `runner.import_duplicate`.
6. Activity/evidence:
   - add activity event for imports.
   - record source URI without secrets.
   - never log raw YAML when it may contain secrets.

## File Targets

- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-distributor-client/src/lib.rs`
- `crates/greentic-desktop-cli/src/lib.rs` only if shared import helpers need to move

## Out of Scope

- Full remote marketplace search.
- Authentication UI for private registries.
- Importing non-YAML bundle formats.

## Acceptance Tests

1. `POST /api/v1/runners/import` with valid YAML creates a runner visible in `GET /api/v1/runners`.
2. Invalid YAML returns a structured 400 with `runner.import_invalid_yaml`.
3. `POST /api/v1/runners/import` with `repo://...` calls distributor resolution and imports the cached YAML.
4. Unsupported URL schemes fail before any network/distributor call.
5. Duplicate runner ids do not silently overwrite existing runners.
6. Imported runners are immediately available for run, edit, MCP exposure, and export.

## Done Means

The GUI has a production import API that shares the same import, validation, distributor, and persistence semantics as the CLI.

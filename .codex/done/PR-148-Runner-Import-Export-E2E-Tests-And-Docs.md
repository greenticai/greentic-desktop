# PR-148 - Runner Import Export E2E Tests And Docs

## Goal

Add automated coverage and documentation for the new runner import/export loop.

## User Outcome

The import/export feature is not just visible in the GUI; it is proven end-to-end and documented clearly enough for users to move runners between machines or stores.

## Current Evidence

- Recent regressions slipped through because tests exercised models without proving user-facing flows.
- CLI import/export exists, but GUI import/export needs browser-level and API-level coverage.
- Distributor URI import must be tested separately from local file upload because it depends on `greentic-distributor-client` resolution behavior.

## Scope

1. API tests:
   - import valid YAML content.
   - reject invalid YAML.
   - reject unsupported URL scheme.
   - export imported YAML.
   - re-import exported YAML into a clean runner home.
2. Distributor tests:
   - use a local fake/cached distributor artifact for `repo://` or `store://` where possible.
   - assert `greentic-distributor-client` is called through the real resolution path, not duplicated ad hoc code.
3. Frontend tests:
   - create page shows three creation choices.
   - upload YAML flow imports and navigates to runner page/list.
   - URL flow validates scheme and submits supported sources.
   - My runners export action triggers a YAML download.
4. Live smoke:
   - import one example YAML.
   - run or validate it if capabilities are available.
   - export it.
   - compare the exported YAML can parse back into a runner package.
5. Docs:
   - add "Import a runner" section.
   - add "Export a runner" section.
   - document supported URI schemes: `oci://`, `store://`, `repo://`, `file://`.
   - explain what is and is not included in exported YAML.
   - provide CLI equivalents:
     - `greentic-desktop --import ...`
     - `greentic-desktop --export ... --out ...`

## File Targets

- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-cli/src/lib.rs`
- `frontend/automate-hub/src/routes/create.tsx`
- `frontend/automate-hub/src/routes/runners.tsx`
- `frontend/automate-hub/src/**/*.test.*` or existing frontend test location
- `docs/runners.md`
- `docs/import-export.md`

## Out of Scope

- Live remote registry integration against production services.
- Private registry credential management.
- `.gtpack` import/export.

## Acceptance Tests

1. `cargo test -p greentic-desktop-gui runner_import_export` covers API import/export and re-import.
2. Frontend test proves the create page has the third option and can submit local YAML.
3. Frontend test proves My runners can download YAML.
4. Unsupported source schemes are rejected with a clear user-visible error.
5. Documentation includes local upload, distributor URI import, and export/download instructions.
6. `ci/local_check.sh` includes the relevant API/frontend tests so GitHub Actions cannot miss this flow.

## Done Means

The runner import/export loop is implemented, covered, and documented as a real user workflow rather than a CLI-only capability.

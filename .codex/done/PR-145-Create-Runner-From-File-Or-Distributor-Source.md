# PR-145 - Create Runner From File Or Distributor Source

## Goal

Extend the "How do you want to create your runner?" page with a third option: "Provide a runner file".

## User Outcome

A user who already has a runner YAML can import it directly from local disk or from a Greentic distributor URI instead of recreating it through prompting or recording.

## Current Evidence

- The CLI already supports `--import (PATH|file://PATH|oci://REF|store://ID|repo://REF)`.
- The GUI creation page currently only supports prompt and recording modes.
- Users need a visible product path for sharing, importing, and reusing runner YAML files.

## Scope

1. Add a third card to `ChooseMode`:
   - label: `Provide a runner file`.
   - icon: use a file/upload-related lucide icon.
   - copy: explain that the runner can come from local YAML or a distributor URL.
2. Extend create mode state:
   - `Mode = null | "prompt" | "record" | "file"`.
   - URL query support: `?mode=file`.
3. Add a new `RunnerFileWizard`:
   - tabs or segmented control for `Upload YAML` and `Import URL`.
   - local upload accepts `.yaml` and `.yml`.
   - URL import accepts only `oci://`, `store://`, `repo://`, and optionally `file://`.
   - show validation errors inline.
   - after successful import, navigate to the imported runner detail/view page rather than `/runners` only.
4. Upload behavior:
   - read the local file in browser.
   - POST YAML content to the GUI import API.
   - show parsed runner name/id before final confirmation if the backend returns it.
5. URL behavior:
   - POST source URI to the GUI import API.
   - show source, resolved artifact path/URI, runner id, and runner name after import.
6. UX states:
   - loading/importing.
   - invalid YAML.
   - unsupported URI scheme.
   - distributor resolution failure.
   - duplicate runner id with replace/keep-both handling if supported by backend.

## File Targets

- `frontend/automate-hub/src/routes/create.tsx`
- `frontend/automate-hub/src/lib/api.ts`
- `frontend/automate-hub/src/lib/types.ts`
- `frontend/automate-hub/src/routeTree.gen.ts` if route generation requires it

## Out of Scope

- Editing imported runners after import; that stays in the runner edit flow.
- Importing packaged `.gtpack` files.
- Building a runner marketplace browser.

## Acceptance Tests

1. Create page displays three options: describe, record, and provide a runner file.
2. Selecting `Provide a runner file` opens an import wizard without showing prompt/record controls.
3. Uploading a valid YAML imports the runner and navigates to its runner page.
4. Uploading invalid YAML shows the backend validation error and does not create a runner.
5. Providing `oci://`, `store://`, or `repo://` source starts distributor-backed import.
6. Providing `https://`, `ftp://`, or arbitrary text is rejected before import with a clear message.

## Done Means

The GUI has a first-class, user-visible path for creating a runner from an existing YAML file or Greentic distributor source.

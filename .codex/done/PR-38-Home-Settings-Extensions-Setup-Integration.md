# PR-38 - Home, Settings, Extensions, and Setup Integration

## Goal

Connect the Automate Hub home and settings pages to real runtime setup, permissions, remote/local extension store, extension trust, and LLM configuration state.

## User Outcome

The setup checklist is not decorative. A user can see what is missing, install/test supported extensions, check permissions, configure LLM settings, and open logs/developer paths from the GUI.

## Current State

- Home checklist is hard-coded.
- Settings checklist is hard-coded.
- Extensions list is hard-coded and includes items not currently represented one-to-one in the Rust extension registry.
- Runtime supports built-in extension install/list/verify/sidecar preparation.
- PR-47 through PR-53 define the remote extension package format, distributor resolution, local extension store, friendly store index, GHCR publish pipeline, signing/trust policy, and extension API.
- Platform permission models exist in platform/macOS/Linux/IO crates but are not exposed through runtime GUI APIs.
- LLM planning currently uses a heuristic/default client; provider configuration is not exposed through settings.

## Scope

1. Replace home setup checklist with `/api/v1/setup/checklist`.
2. Replace settings extension list with `/api/v1/extensions/recommended` and `/api/v1/extensions/installed`.
3. Add search, install, update, remove, enable, disable, verify, and health buttons backed by API endpoints.
4. Add permission diagnostics for Windows/macOS/Linux.
5. Add LLM settings read/write model.
6. Add log and runtime path actions for advanced settings.
7. Surface remote extension source, version, publisher, digest, permissions, trust status, and platform compatibility in the UI.

## Required Backend Work

### Setup Checklist Service

Add a runtime method that returns:

- runtime initialized
- current platform detected
- screen capture permission state
- accessibility permission state
- input-control permission state
- browser adapter installed
- vision fallback installed
- MCP server running

Each item needs:

- `id`
- `label`
- `status`: `ready | warning | missing | unsupported`
- `help`
- optional `action`: `install_extension | open_system_settings | start_mcp | open_docs`

### Extension Store Mapping

Map current built-in and remote extension IDs to UI-friendly names:

- `greentic.desktop.playwright`
- `greentic.desktop.windows-ui`
- `greentic.desktop.macos.ax`
- `greentic.desktop.linux.x11`
- `greentic.desktop.linux.wayland`
- `greentic.desktop.java-accessibility`
- `greentic.desktop.terminal.tn3270`
- `greentic.desktop.vision`

This mapping must come from the store index and local installed-extension store once PR-50 and PR-49 are available. Before those PRs are implemented, use the built-in registry as a compatibility source.

If the UI bundle has conceptual extensions not yet implemented, show them as `available: false` or hide them until implemented. Do not show installable buttons for unsupported adapters unless the backend can act.

### Remote Extension Install Flow

The UI install/update flow must align with PR-48 through PR-53:

1. User clicks install on a friendly extension card.
2. GUI sends `source` as a friendly ID or `store://...` alias.
3. Backend resolves the alias through the extension store index.
4. Backend downloads through `greentic-distributor-client`.
5. Backend verifies digest, signature, trusted publisher, permissions, platform compatibility, and SBOM/provenance requirements.
6. Backend installs into the local extension store and updates `installed.lock`.
7. UI displays installed version, health status, enabled/disabled state, and any restart requirement.

The UI should not implement GHCR, OCI, repo, or file download logic directly.

### Trust and Permission Prompting

Before installing an extension with high-impact permissions, show a confirmation panel with:

- publisher and signature status
- source URI and digest in advanced details
- requested permissions in plain English
- platform compatibility
- capabilities the extension will add
- whether the extension is official, tenant-provided, local unsigned draft, or blocked

Blocked installs should show the trust-policy reason from PR-52 and must not show a generic failure.

### Permission Fix Actions

Implement OS-specific action responses:

- macOS: explain Accessibility, Screen Recording, Input Monitoring; optionally open System Settings deep links if safe
- Windows: explain UI Automation/input permissions and Defender prompts if relevant
- Linux: explain X11/Wayland/portal/AT-SPI status

The API should return instructions if it cannot automatically fix the state.

### LLM Settings

Add config fields and endpoints:

```http
GET /api/v1/settings/llm
PUT /api/v1/settings/llm
POST /api/v1/settings/llm/test
```

Fields:

- provider
- model
- endpoint/base URL if applicable
- secret reference, never raw secret value
- mode: `heuristic | remote`

## Frontend Work

- Home route fetches setup checklist and runtime info.
- Settings route fetches setup checklist, extensions, and LLM settings.
- Buttons call real endpoints and show loading/error/success states.
- Extension install/update actions show progress states: resolving, downloading, verifying, installing, health-checking, complete, failed.
- Remove static setup arrays from production route code.
- Keep visible text non-technical.

## Acceptance Criteria

- Home checklist reflects real runtime state.
- Settings extension install/test actions call backend APIs.
- Settings can browse recommended remote extensions and search the extension store.
- Extension install uses friendly IDs/store aliases and never exposes OCI details in the normal path.
- Extension trust/permission prompts are shown before high-impact installs.
- Installed extensions can be updated, removed, enabled, disabled, verified, and health-checked.
- Unsupported extensions are clearly marked or hidden.
- Permission fix actions show OS-appropriate guidance.
- LLM settings can be saved and tested without exposing raw secrets.
- Advanced settings show real runner storage path and MCP bind address.

## Test Plan

- Unit tests for setup checklist generation on mocked platform states.
- Extension API tests for install/list/verify.
- Frontend tests or smoke checks for loading/error states.
- Manual macOS/Windows/Linux checklist validation.

## Risks

- Permission detection can be partially implemented on some OSes. The API should distinguish `unknown` from `ready`.
- LLM provider settings touch secrets; keep raw keys out of logs, API responses, and persisted plaintext where possible.

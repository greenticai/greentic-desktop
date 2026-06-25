# PR-37 - Local GUI API and Type Contract

## Goal

Create a local JSON API between the Automate Hub frontend and the Rust backend. Replace demo-data assumptions with typed API responses while keeping the UI usable during incremental backend integration.

## User Outcome

The GUI reflects the real local Greentic Desktop runtime: version, setup state, installed adapters/extensions, recording sessions, runners, MCP server status, and recent activity.

## Current State

- Frontend data comes from `src/lib/demo-data.ts`.
- Backend functions exist but are exposed mostly through CLI text output:
  - runtime info
  - extension install/list/verify
  - runner list
  - prompt planning
  - recording lifecycle
  - MCP serve
- There is no stable JSON schema shared between frontend and backend.

## Scope

1. Add `/api/*` route handling to the GUI host.
2. Define versioned JSON response schemas for every page-level data dependency.
3. Add frontend API client functions and TypeScript types.
4. Keep mock fallback only for frontend development, not production runtime.
5. Add error response conventions that the UI can render consistently.

## API Contract

### Health and Runtime

```http
GET /api/v1/health
GET /api/v1/runtime/info
GET /api/v1/activity
```

Runtime info should include:

- app version
- OS/platform
- runtime home
- evidence store path
- config summary
- current GUI bind URL
- installed core adapter IDs

### Setup

```http
GET /api/v1/setup/checklist
POST /api/v1/setup/fix
```

Checklist items map to the UI home/settings cards:

- runtime home exists
- browser automation extension installed
- OS screen capture permission
- OS accessibility permission
- keyboard/mouse control permission
- MCP server state

### Extensions

These endpoints are the GUI-facing contract over the remote extension work in PR-47 through PR-53. The GUI must not know how to pull GHCR artifacts directly; it calls the local backend, which resolves aliases, calls `greentic-distributor-client`, verifies signatures/trust policy, installs into the local extension store, and returns progress/status DTOs.

```http
GET /api/v1/extensions/recommended
GET /api/v1/extensions/installed
GET /api/v1/extensions/search?q=browser
GET /api/v1/extensions/{id}
GET /api/v1/extensions/{id}/versions
POST /api/v1/extensions/install
POST /api/v1/extensions/{id}/update
POST /api/v1/extensions/{id}/remove
POST /api/v1/extensions/{id}/enable
POST /api/v1/extensions/{id}/disable
POST /api/v1/extensions/{id}/verify
POST /api/v1/extensions/{id}/health
```

Install requests accept friendly IDs, `store://`, `oci://`, `repo://`, or `file://` sources:

```json
{
  "source": "store://greentic.desktop.playwright",
  "version": "latest"
}
```

Install/update responses must support progress states used by the UI:

```json
{
  "id": "greentic.desktop.playwright",
  "status": "installing",
  "phase": "verifying",
  "version": "1.0.0",
  "source": "oci://ghcr.io/greenticai/greentic-desktop/extensions/playwright:1.0.0",
  "digest": "sha256:...",
  "publisher": "greenticai",
  "permissions": ["network"],
  "capabilities": ["web.goto", "web.click", "web.fill"]
}
```

### Runners

```http
GET /api/v1/runners
GET /api/v1/runners/{id}
POST /api/v1/runners/{id}/test
POST /api/v1/runners/{id}/publish
POST /api/v1/runners/{id}/run
```

### Recordings

```http
GET /api/v1/recordings
GET /api/v1/recordings/{session_id}
```

Detailed recording lifecycle endpoints are planned in PR-40.

### MCP

```http
GET /api/v1/mcp/status
GET /api/v1/mcp/tools
POST /api/v1/mcp/start
POST /api/v1/mcp/stop
POST /api/v1/mcp/restart
POST /api/v1/mcp/tools/{name}/test
POST /api/v1/mcp/tools/{name}/disable
```

## Response Shape

All successful responses:

```json
{
  "ok": true,
  "data": {}
}
```

All errors:

```json
{
  "ok": false,
  "error": {
    "code": "runtime.extension_not_found",
    "message": "Extension not found",
    "details": {}
  }
}
```

## Technical Plan

### Backend Types

Add serializable DTOs in a GUI/API crate:

- `ApiResponse<T>`
- `ApiError`
- `RuntimeInfoDto`
- `SetupChecklistDto`
- `ExtensionDto`
- `ExtensionStoreEntryDto`
- `ExtensionInstallProgressDto`
- `ExtensionVerificationDto`
- `RunnerSummaryDto`
- `RecordingSummaryDto`
- `McpStatusDto`
- `ActivityEventDto`

Use stable strings for status values because the frontend will depend on them.

### Frontend Client

Add:

```text
frontend/automate-hub/src/lib/api.ts
frontend/automate-hub/src/lib/types.ts
```

Replace direct imports from `demo-data.ts` route by route in later PRs.

### Error Handling

The API should never return Rust debug strings as machine codes. Map known errors to stable codes:

- `runtime.io`
- `runtime.security`
- `extension.not_found`
- `extension.resolve_failed`
- `extension.download_failed`
- `extension.signature_invalid`
- `extension.publisher_untrusted`
- `extension.permission_denied`
- `extension.platform_unsupported`
- `recording.invalid_state`
- `planner.needs_clarification`
- `mcp.server_unavailable`

## Acceptance Criteria

- `/api/v1/health` and `/api/v1/runtime/info` return JSON.
- The frontend has a typed API client.
- The UI can render API loading, empty, and error states.
- Demo data is clearly isolated to development mode.
- Backend tests cover response serialization and error mapping.

## Test Plan

- Rust unit tests for DTO serialization.
- GUI host integration test for several `/api/v1/*` endpoints.
- Frontend typecheck.
- Browser smoke test of initial page loading real runtime info.

## Risks

- Duplicated Rust and TypeScript types can drift; consider generating TypeScript from Rust DTOs in a later PR if drift becomes a problem.

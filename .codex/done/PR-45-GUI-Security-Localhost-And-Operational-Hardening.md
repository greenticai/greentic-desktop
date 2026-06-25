# PR-45 - GUI Security, Localhost Boundaries, and Operational Hardening

## Goal

Harden the browser-based GUI so local automation, remote extension installation, evidence, secrets, and MCP controls are not exposed beyond the user's machine or to arbitrary web pages.

## User Outcome

The GUI is convenient but still safe: it only listens on loopback, rejects cross-site requests, protects sensitive actions, redacts secrets, and logs enough information to troubleshoot issues.

## Current State

- No GUI server exists yet.
- Existing security model covers runner policy, secrets, approvals, and redaction.
- Evidence and screenshots can be sensitive.
- Remote extensions may request powerful permissions and install native binaries or sidecars.
- Browser-based localhost apps need CSRF/origin protections.

## Scope

1. Add local GUI session token.
2. Enforce loopback-only binding by default.
3. Add Origin/Referer/CSRF checks for mutating API calls.
4. Add security headers for served UI.
5. Harden evidence/artifact endpoints.
6. Enforce extension trust-policy display and confirmation for remote installs.
7. Add structured GUI logs.
8. Add shutdown and stale-process behavior.

## Security Requirements

### Binding

- Default GUI bind must be `127.0.0.1:<dynamic>`.
- Binding to non-loopback addresses requires explicit flag and warning.
- MCP bind remains separately configurable.

### Session Token

On GUI server start:

- generate random token
- include token in opened URL fragment or query parameter
- frontend stores token in memory
- all mutating API calls send token header, for example `X-Greentic-GUI-Token`

Avoid writing token into logs after startup.

### CSRF/Origin

For `POST`, `PATCH`, `PUT`, `DELETE`:

- require token header
- require Origin to match the GUI host if Origin is present
- reject cross-site content types unless explicitly accepted

### Headers

Serve UI with:

- `Content-Security-Policy`
- `X-Content-Type-Options: nosniff`
- `Referrer-Policy: no-referrer`
- restrictive cache policy for API responses

### Evidence

Artifact paths must:

- use opaque IDs, not raw filesystem paths
- reject path traversal
- redact secrets before serialization
- avoid exposing full local paths unless developer mode is enabled

### Remote Extension Installation

GUI extension install/update/remove endpoints must enforce the same trust policy as CLI extension installation:

- unsigned production packages are blocked unless policy allows them
- local unsigned drafts are only allowed in development mode
- untrusted publishers are blocked
- permission prompts are required for screen capture, keyboard/mouse, filesystem write, network, and native binary execution
- source URI, digest, publisher, signature status, and SBOM presence are available in advanced details

The browser UI should display trust and permission prompts, but the backend remains authoritative. A user bypassing the frontend must still be blocked by backend policy.

## Operational Requirements

- Clear startup logs:
  - version
  - GUI URL
  - runtime home
  - MCP bind if running
- Clear error logs for browser-open failures.
- Graceful shutdown on Ctrl-C.
- Optional idle timeout policy can be planned but should not surprise users initially.

## Acceptance Criteria

- GUI and API bind only to loopback by default.
- Mutating API calls without token fail.
- Cross-origin mutating requests fail.
- Evidence artifact endpoint cannot read arbitrary files.
- Extension install/update requests cannot bypass distributor resolution, signature verification, trust policy, or permission approval.
- Secrets are never returned raw from settings, evidence, activity, or logs.
- Browser UI still works with the token flow.

## Test Plan

- API tests for token required/missing/wrong.
- Origin rejection tests.
- Path traversal tests for evidence artifacts.
- Extension trust-policy tests for blocked unsigned, blocked untrusted publisher, blocked high-risk permission, and allowed local draft mode.
- Security header tests on static assets and API responses.
- Manual browser smoke test.

## Risks

- Too strict a CSP may break the current bundled UI. Start with a policy that supports the built static app and tighten iteratively.

# PR-119 - MCP Transport Security and Auth

## Goal

Make the MCP endpoint transport and authentication story coherent and secure.

## User Outcome

Local agents can use stdio safely, while HTTP MCP access requires explicit authentication and cannot be called by arbitrary local web pages.

## Current Evidence

- GUI advertises `mcp_bind` but current endpoint is not a valid MCP transport.
- There is no MCP auth/token despite security claims.
- Browser GUI token handling exists separately from MCP.

## Scope

1. Decide transport defaults:
   - stdio is default and recommended for desktop agents.
   - HTTP transport is opt-in.
2. Stdio:
   - per-client process.
   - no network bind.
3. HTTP:
   - require bearer token or signed local session token.
   - reject missing/invalid auth.
   - reject browser-origin cross-site calls unless explicitly allowed.
   - document localhost threat model.
4. Token management:
   - generate token in runtime home with `0600` permissions.
   - rotate token command.
   - do not log token.
5. GUI:
   - settings should show MCP status and auth mode without exposing secrets.
6. Tests:
   - unauthorized HTTP calls fail.
   - authorized calls succeed.
   - stdio does not require HTTP token.

## File Targets

- `crates/greentic-desktop-mcp/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-desktop-security/src/lib.rs`
- `docs/*`

## Out of Scope

- OAuth for remote hosted MCP.
- Multi-user RBAC.

## Acceptance Tests

1. HTTP MCP initialize without token returns auth error.
2. HTTP MCP initialize with valid token succeeds.
3. Token is stored with owner-only permissions on Unix.
4. Logs and evidence do not include token.
5. Stdio MCP E2E from PR-115 still passes.

## Done Means

The MCP endpoint is both protocol-correct and not accidentally open.

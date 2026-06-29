# PR-115 - Spec-Compliant MCP Server Stdio and HTTP

## Goal

Replace the hand-rolled MCP endpoints with a spec-compliant MCP server that real MCP clients can initialize, list tools from installed runners, and call those tools.

## User Outcome

Claude Desktop, Cursor, MCP Inspector, and SDK clients can connect to Greentic Desktop, complete the MCP initialize lifecycle, see real runners as tools, call them, and receive structured outputs/errors/evidence.

## Current Evidence

- `crates/greentic-desktop-runtime/src/lib.rs` contains a fixed-size TCP request parser, hardcoded `crm.create_customer`, and no real `tools/call`.
- `crates/greentic-desktop-gui/src/lib.rs` has a better but still non-compliant MCP HTTP-ish handler:
  - no `initialize` handshake.
  - hardcoded response id.
  - single 8192-byte read.
  - string matching on request body.
  - no session state.
- Existing tests mostly validate JSON snippets, not real MCP client behavior.

## Scope

1. Choose and add a real MCP implementation path:
   - Preferred: use `rmcp` if it is viable for the current Rust MSRV and target platforms.
   - Fallback only if `rmcp` cannot be adopted: implement JSON-RPC 2.0 MCP lifecycle explicitly with tests against MCP Inspector-compatible payloads.
2. Implement stdio transport first:
   - `greentic-desktop mcp serve --stdio`.
   - support `initialize`, `notifications/initialized`, `tools/list`, `tools/call`, and protocol errors.
   - echo request ids exactly.
   - support request bodies larger than one fixed buffer.
3. Implement Streamable HTTP only after stdio is correct:
   - no body substring matching.
   - JSON-RPC id echo.
   - session initialization.
   - documented auth requirement from PR-119.
4. Wire `tools/list` to real installed/published runner manifests:
   - no hardcoded example tools.
   - include JSON schema from runner input schema.
   - hide disabled or policy-blocked runners.
5. Wire `tools/call` to `execute_runner`/replay:
   - map MCP tool args to runner inputs/secrets.
   - return outputs and evidence refs on success.
   - return structured MCP tool errors with failed-step diagnostics from replay.
6. Remove or quarantine legacy fake MCP handlers:
   - runtime fake TCP handler must be deleted or replaced.
   - GUI fake handler must delegate to the shared MCP implementation.
7. Add a CLI smoke command:
   - `greentic-desktop mcp inspect --stdio --runner <id>` or equivalent local test helper.

## File Targets

- `crates/greentic-desktop-mcp/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-cli/src/lib.rs`
- `Cargo.toml`
- `.github/workflows/*` if a CI MCP smoke test is added.

## Out of Scope

- Remote hosted MCP service.
- OAuth provider implementation beyond local authenticated HTTP from PR-119.
- Desktop automation improvements.

## Acceptance Tests

1. A real MCP client fixture sends `initialize`; response contains protocol version, server info, and capabilities.
2. `tools/list` returns tools from runner files in a temporary runtime home, not a hardcoded example.
3. JSON-RPC response ids exactly match string and numeric request ids.
4. `tools/call` executes a real local web runner and returns structured output plus evidence ref.
5. Invalid method returns a JSON-RPC method-not-found error.
6. Large requests beyond 8192 bytes are accepted or rejected with a protocol error, never truncated.
7. MCP Inspector-compatible fixture test passes in CI.

## Done Means

An external MCP client can call a Greentic runner without any Greentic-specific assumptions.

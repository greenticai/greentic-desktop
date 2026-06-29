# PR-128 - Replace Hand-Rolled HTTP And MCP With Axum And RMCP

## Goal

Replace manual TCP/HTTP parsing and custom MCP JSON-RPC rendering with `axum` and `rmcp`.

## User Outcome

MCP clients get a spec-compliant server with correct initialize/tools/list/tools/call behavior, JSON-RPC id correlation, stdio support, Streamable HTTP support, and safer HTTP handling.

## Current Evidence

- GUI and MCP code still contain manual `TcpListener` loops, string-built HTTP responses, and custom JSON-RPC renderers.
- Current behavior works for tests but is brittle for real MCP clients and streaming transports.
- The official Rust MCP SDK (`rmcp`) should own protocol semantics instead of local string formatting.

## Scope

1. Add `rmcp`, `axum`, `hyper`, and `tokio` server integration.
2. Implement a single MCP service over:
   - stdio transport.
   - Streamable HTTP transport.
   - GUI-managed localhost HTTP transport with existing GUI token policy.
3. Replace hand-rendered MCP JSON strings with `rmcp` request/response types.
4. Replace manual HTTP response building with `axum` routes for GUI APIs where practical.
5. Preserve current GUI route contract and token validation.
6. Add graceful shutdown and concurrency limits through `tokio`.

## File Targets

- `crates/greentic-desktop-mcp/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-cli/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `Cargo.toml`

## Out of Scope

- Changing runner schemas.
- Adding non-local network exposure.

## Acceptance Tests

1. An `rmcp` client can initialize, list tools, and call a saved runner over stdio.
2. An `rmcp` client can initialize, list tools, and call a saved runner over Streamable HTTP.
3. JSON-RPC response ids match request ids for concurrent requests.
4. GUI HTTP MCP still rejects missing/invalid tokens and non-local origins.
5. Existing runner/MCP tests are migrated away from string-only assertions where `rmcp` types are available.

## Done Means

There is one spec-compliant MCP implementation, not two hand-rolled protocol stacks.

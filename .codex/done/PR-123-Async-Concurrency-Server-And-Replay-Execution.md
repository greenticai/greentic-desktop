# PR-123 - Async Concurrency Server and Replay Execution

## Goal

Introduce a real concurrency model so MCP/GUI servers and replay execution do not block each other.

## User Outcome

One slow runner does not freeze the GUI or prevent MCP clients from connecting/listing tools.

## Current Evidence

- Synchronous `TcpListener` loops and fixed-buffer reads exist.
- Replay execution is synchronous.
- A slow app/LLM call can block request handling.

## Scope

1. Add async runtime or bounded thread pool:
   - preferred: `tokio` for MCP/HTTP/LLM.
   - if avoiding async for now, add explicit thread pool with timeouts.
2. Server request handling:
   - concurrent MCP sessions.
   - concurrent GUI API requests where safe.
3. Replay execution:
   - run blocking OS automation off the async reactor.
   - per-run timeout.
   - cancellation token for stop.
4. Resource limits:
   - max concurrent runs.
   - queue overflow error.
   - per-run logs/evidence.
5. Tests:
   - two clients list tools while one long replay runs.
   - timeout returns structured failure.

## File Targets

- `crates/greentic-desktop-mcp/src/lib.rs`
- `crates/greentic-desktop-gui/src/lib.rs`
- `crates/greentic-desktop-replay/src/lib.rs`
- `crates/greentic-desktop-runtime/src/lib.rs`
- `Cargo.toml`

## Out of Scope

- Distributed worker queues.
- Multi-machine orchestration.

## Acceptance Tests

1. A simulated slow replay does not block `tools/list`.
2. Two MCP clients can initialize concurrently.
3. Replay timeout returns failed-step diagnostics and evidence.
4. Server shuts down cleanly with in-flight request cancellation.

## Done Means

The runtime has an explicit, tested concurrency boundary.

#!/usr/bin/env bash
set -euo pipefail

failures=0

check_pattern() {
  local label="$1"
  local pattern="$2"
  local allowed_paths="$3"
  local output
  output="$(
    rg -n "$pattern" crates --glob '*.rs' 2>/dev/null \
      | grep -Ev "$allowed_paths" || true
  )"
  if [ -n "$output" ]; then
    printf 'handrolled scripting check failed: %s\n%s\n' "$label" "$output" >&2
    failures=$((failures + 1))
  fi
}

check_pattern \
  "curl subprocesses must use reqwest or the shared HTTP client" \
  'Command::new\("curl"\)|run_command\("curl"' \
  '^$'

check_pattern \
  "macOS scripting must not spread beyond the documented macOS migration file" \
  'Command::new\("osascript"\)|run_command\("osascript"|run_osascript\(' \
  '^crates/greentic-desktop-macos/src/lib.rs:'

check_pattern \
  "screen capture subprocesses must use xcap" \
  'Command::new\("screencapture"\)|run_command\("screencapture"|screencapture did not create' \
  '^$'

check_pattern \
  "X11 command utilities must not spread beyond the documented Linux migration file" \
  'Command::new\("wmctrl"\)|run_command\("wmctrl"|Command::new\("xdotool"\)|run_command\("xdotool"' \
  '^crates/greentic-desktop-linux/src/lib.rs:'

check_pattern \
  "generated PowerShell UIA must not spread beyond the documented Windows migration file" \
  'Command::new\("powershell"\)|run_powershell\(|UIAutomationClient|UIAutomationTypes' \
  '^crates/greentic-desktop-windows/src/lib.rs:'

check_pattern \
  "manual MCP HTTP server loops must stay out of runtime code now that axum/rmcp are available" \
  'read_http_request|handle_mcp_connection|HTTP/1\.1 200 OK|jsonrpc.:.2\.0' \
  '^crates/greentic-desktop-gui/src/lib.rs:|^crates/greentic-desktop-mcp/src/lib.rs:|^crates/greentic-desktop-web/src/lib.rs:|^crates/greentic-desktop-llm/src/lib.rs:'

if [ "$failures" -ne 0 ]; then
  exit 1
fi

printf 'no-handrolled scripting check passed.\n'

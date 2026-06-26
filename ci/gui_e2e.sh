#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo build --bin greentic-desktop

export GREENTIC_DESKTOP_BINARY="${GREENTIC_DESKTOP_BINARY:-$ROOT/target/debug/greentic-desktop}"

if [ ! -x "$GREENTIC_DESKTOP_BINARY" ] && [ -x "$GREENTIC_DESKTOP_BINARY.exe" ]; then
  export GREENTIC_DESKTOP_BINARY="$GREENTIC_DESKTOP_BINARY.exe"
fi

npm --prefix frontend/automate-hub run e2e -- --grep "${GREENTIC_E2E_GREP:-@smoke}" "$@"

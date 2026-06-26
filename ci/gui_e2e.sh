#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if ! node -e 'const [major, minor] = process.versions.node.split(".").map(Number); if (!((major === 20 && minor >= 19) || major >= 22)) process.exit(1);' >/dev/null 2>&1; then
  printf 'GUI E2E requires Node.js 20.19+ or 22.12+. Found: %s\n' "$(node -v 2>/dev/null || printf 'not installed')" >&2
  exit 1
fi

if [ ! -d frontend/automate-hub/node_modules ]; then
  if [ -f frontend/automate-hub/package-lock.json ]; then
    npm --prefix frontend/automate-hub ci
  else
    npm --prefix frontend/automate-hub install
  fi
fi

npm --prefix frontend/automate-hub run build

cargo build --bin greentic-desktop

export GREENTIC_DESKTOP_BINARY="${GREENTIC_DESKTOP_BINARY:-$ROOT/target/debug/greentic-desktop}"

if [ ! -x "$GREENTIC_DESKTOP_BINARY" ] && [ -x "$GREENTIC_DESKTOP_BINARY.exe" ]; then
  export GREENTIC_DESKTOP_BINARY="$GREENTIC_DESKTOP_BINARY.exe"
fi

npm --prefix frontend/automate-hub run e2e -- "$@" --grep "${GREENTIC_E2E_GREP:-@smoke}"

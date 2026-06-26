#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

if [ "${GREENTIC_DESKTOP_REAL_DESKTOP:-0}" != "1" ] \
  && [ "${GREENTIC_DESKTOP_REAL_JAVA:-0}" != "1" ] \
  && [ "${GREENTIC_DESKTOP_REAL_LLM:-0}" != "1" ]; then
  cat <<'EOF'
Skipping real desktop E2E tests.

Set one or more explicit opt-in flags when the host is prepared:
  GREENTIC_DESKTOP_REAL_DESKTOP=1  real OS calculator/app automation
  GREENTIC_DESKTOP_REAL_JAVA=1     real Java accessibility fixture
  GREENTIC_DESKTOP_REAL_LLM=1      real configured LLM provider checks
EOF
  exit 0
fi

case "$(uname -s)" in
  Darwin) project="desktop-real-macos" ;;
  Linux) project="desktop-real-linux" ;;
  MINGW*|MSYS*|CYGWIN*) project="desktop-real-windows" ;;
  *) project="desktop-real-linux" ;;
esac

export GREENTIC_E2E_GREP="${GREENTIC_E2E_GREP:-@manual|@desktop-real|@java-real}"

bash ci/gui_e2e.sh --project "$project"

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

export GREENTIC_E2E_GREP="${GREENTIC_E2E_GREP:-@smoke}"

bash ci/gui_e2e.sh --project chromium-smoke

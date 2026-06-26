#!/usr/bin/env bash
set -euo pipefail

cat <<EOF
# Greentic Desktop E2E Release Gate

| Suite | Status | Gate |
| --- | --- | --- |
| GUI smoke/setup | ${GREENTIC_E2E_SMOKE_STATUS:-not reported} | required |
| Functional mock automation | ${GREENTIC_E2E_FUNCTIONAL_STATUS:-not reported} | required for release branch/main |
| Real desktop automation | ${GREENTIC_E2E_REAL_DESKTOP_STATUS:-manual or skipped} | manual release evidence |
| Real Java accessibility | ${GREENTIC_E2E_REAL_JAVA_STATUS:-manual or skipped} | manual release evidence |
| Real LLM provider | ${GREENTIC_E2E_REAL_LLM_STATUS:-manual or skipped} | manual release evidence |

Required suites must not depend on public websites, desktop permissions, or live LLM secrets.
Manual suites require prepared hosts and should attach Playwright reports from
\`frontend/automate-hub/playwright-report\` and \`frontend/automate-hub/test-results\`.
EOF

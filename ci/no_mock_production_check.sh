#!/usr/bin/env bash
set -euo pipefail

failures=0

check_absent() {
  local label="$1"
  shift
  local output
  output="$("$@" 2>/dev/null || true)"
  if [ -n "$output" ]; then
    printf 'no-mock check failed: %s\n%s\n' "$label" "$output" >&2
    failures=$((failures + 1))
  fi
}

check_absent \
  "capability-only replay adapter must not exist" \
  rg -n "CapabilityOnlyAdapter" crates --glob '*.rs'

check_absent \
  "fake recording backend must not be used" \
  rg -n "FakeRecordingBackend::ready|fake backend heartbeat" crates --glob '*.rs'

check_absent \
  "generic successful fake step messages are forbidden" \
  rg -n "step accepted|completed by configured backend\"\\.to_owned\\(\\).*success: true" crates --glob '*.rs'

check_absent \
  "production code must not instantiate StaticAdapter" \
  rg -n "StaticAdapter::new" crates --glob '*.rs' --glob '!crates/greentic-desktop-adapter/src/lib.rs'

if [ "$failures" -ne 0 ]; then
  exit 1
fi

printf 'no-mock production check passed.\n'

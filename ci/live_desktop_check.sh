#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cargo metadata --no-deps --format-version 1 | sed -n 's/.*"workspace_root":"\([^"]*\)".*/\1/p' | head -n 1)"
cd "$ROOT"

SUITE="${GREENTIC_LIVE_SUITE:-auto}"
ARTIFACT_DIR="${GREENTIC_LIVE_ARTIFACT_DIR:-target/greentic-live-validation}"
mkdir -p "$ARTIFACT_DIR"

log() {
  printf '==> %s\n' "$1"
}

validate_runner() {
  local name="$1"
  shift
  log "live validation: $name"
  "$@" 2>&1 | tee "$ARTIFACT_DIR/$name.log"
}

case "$SUITE" in
  auto|macos|all)
    if [ "$(uname -s)" = "Darwin" ]; then
      workbook_path="${GREENTIC_LIVE_EXCEL_WORKBOOK:-$ARTIFACT_DIR/test.xls}"
      if [ "${GREENTIC_LIVE_EXCEL:-0}" = "1" ]; then
        validate_runner macos-excel \
          cargo run --bin greentic-desktop -- desktop validate \
            --workflow examples/runners/macos-excel-tabs-formula-save.yaml \
            --input "workbook_path=$workbook_path" \
            --input "source_number=10" \
            --expect-file-changed "$workbook_path" \
            --expect-no-modal \
            --json
      else
        printf 'Skipping macOS Excel live fixture. Set GREENTIC_LIVE_EXCEL=1 to run it.\n' \
          | tee "$ARTIFACT_DIR/macos-excel.skipped"
      fi
    fi
    ;;
esac

case "$SUITE" in
  auto|web|terminal|all)
    printf 'Web and terminal live fixtures are not enabled yet; deterministic tests still cover their model paths.\n' \
      | tee "$ARTIFACT_DIR/non-gui.skipped"
    ;;
esac

log "live validation artifacts: $ARTIFACT_DIR"

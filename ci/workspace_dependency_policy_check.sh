#!/usr/bin/env bash
set -euo pipefail

ROOT="$(
  cargo metadata --no-deps --format-version 1 \
    | sed -n 's/.*"workspace_root":"\([^"]*\)".*/\1/p' \
    | head -n 1
)"

cd "$ROOT"

status=0
for manifest in crates/*/Cargo.toml; do
  awk -v file="$manifest" '
    /^\[(dependencies|dev-dependencies|build-dependencies)\]/ {
      section = 1
      next
    }
    /^\[/ {
      section = 0
    }
    section && /^[[:space:]]*[A-Za-z0-9_-]+[[:space:]]*=/ && $0 !~ /workspace[[:space:]]*=[[:space:]]*true/ {
      printf "%s:%d:%s\n", file, NR, $0
      found = 1
    }
    END {
      if (found) {
        exit 1
      }
    }
  ' "$manifest" || status=1
done

if [ "$status" -ne 0 ]; then
  cat >&2 <<'EOF'
Crate manifests must not declare dependency or dev-dependency versions directly.
Add dependency versions to the root Cargo.toml [workspace.dependencies] section
and reference them from crates with `dependency-name.workspace = true`.
EOF
  exit 1
fi

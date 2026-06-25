#!/usr/bin/env bash
set -euo pipefail

header() {
  printf '\n==> %s\n' "$1"
}

workspace_root() {
  cargo metadata --no-deps --format-version 1 \
    | sed -n 's/.*"workspace_root":"\([^"]*\)".*/\1/p' \
    | head -n 1
}

package_name() {
  sed -n '/^\[package\]/,/^\[/ s/^name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$1" \
    | head -n 1
}

is_publishable() {
  ! sed -n '/^\[package\]/,/^\[/p' "$1" | grep -Eq '^[[:space:]]*publish[[:space:]]*=[[:space:]]*false([[:space:]]|$)'
}

publishable_crates() {
  for crate in \
    greentic-desktop-core \
    greentic-desktop-config \
    greentic-desktop-adapter \
    greentic-desktop-extension \
    greentic-desktop-recorder \
    greentic-desktop-llm \
    greentic-desktop-runner-schema \
    greentic-desktop-policy \
    greentic-desktop-planner \
    greentic-desktop-evidence \
    greentic-desktop-registry \
    greentic-desktop-session \
    greentic-desktop-telemetry \
    greentic-desktop-gui-assets \
    greentic-desktop-gui \
    greentic-desktop-security \
    greentic-desktop-replay \
    greentic-desktop-mcp \
    greentic-desktop-runtime \
    greentic-desktop
  do
    if cargo metadata --no-deps --format-version 1 \
      | grep -q "\"name\":\"$crate\""; then
      printf '%s\n' "$crate"
    fi
  done
}

locally_packageable_crates() {
  # Before the first release, crates.io cannot resolve workspace-internal
  # dependencies for downstream crates. Full ordered publish dry-runs happen in
  # .github/workflows/publish.yml after each dependency is published.
  for crate in \
    greentic-desktop-core \
    greentic-desktop-config \
    greentic-desktop-evidence \
    greentic-desktop-registry \
    greentic-desktop-session \
    greentic-desktop-telemetry \
    greentic-desktop-llm \
    greentic-desktop-gui-assets
  do
    if cargo metadata --no-deps --format-version 1 \
      | grep -q "\"name\":\"$crate\""; then
      printf '%s\n' "$crate"
    fi
  done
}

ROOT="$(workspace_root)"
cd "$ROOT"

header "cargo fmt"
cargo fmt --all -- --check

header "cargo clippy"
cargo clippy --all-targets --all-features -- -D warnings

header "cargo test"
cargo test --all-features

header "cargo build"
cargo build --all-features

header "cargo doc"
cargo doc --no-deps --all-features

if [ "${GREENTIC_CHECK_FRONTEND:-0}" = "1" ]; then
  header "frontend build"
  if command -v bun >/dev/null 2>&1; then
    (
      cd frontend/automate-hub
      if [ ! -d node_modules ]; then
        bun install
      fi
      bun run build
      test -f dist/index.html
    )
  elif command -v npm >/dev/null 2>&1; then
    (
      cd frontend/automate-hub
      if [ ! -d node_modules ]; then
        if [ -f package-lock.json ]; then
          npm ci
        else
          npm install
        fi
      fi
      npm run lint
      npm run build
      test -f dist/index.html
    )
  else
    printf 'GREENTIC_CHECK_FRONTEND=1 requires bun or npm.\n' >&2
    exit 1
  fi
fi

CRATES="$(locally_packageable_crates)"
if [ -z "$CRATES" ]; then
  header "cargo package"
  printf 'No publishable crates found.\n'
else
  printf '%s\n' "$CRATES" | while IFS= read -r crate; do
    header "cargo package --no-verify -p $crate"
    cargo package --no-verify -p "$crate" --allow-dirty

    header "cargo package -p $crate"
    cargo package -p "$crate" --allow-dirty

    header "cargo publish --dry-run -p $crate"
    cargo publish -p "$crate" --dry-run --allow-dirty
  done

  header "cargo package"
  printf 'Skipped pre-release package verification for crates with unpublished internal dependencies:\n'
  comm -13 \
    <(printf '%s\n' "$CRATES" | sort) \
    <(publishable_crates | sort) \
    | sed 's/^/  - /'
fi

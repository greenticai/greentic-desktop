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

frontend_node_is_supported() {
  command -v node >/dev/null 2>&1 \
    && node -e 'const [major, minor] = process.versions.node.split(".").map(Number); process.exit(major > 20 || (major === 20 && minor >= 19) ? 0 : 1)'
}

ensure_frontend_node() {
  if frontend_node_is_supported; then
    return
  fi

  if [ -s "${HOME:-}/.nvm/nvm.sh" ]; then
    # shellcheck source=/dev/null
    . "${HOME}/.nvm/nvm.sh"
    for version in 24 22 20; do
      if nvm use "$version" >/dev/null 2>&1 && frontend_node_is_supported; then
        return
      fi
    done
  fi

  printf 'Node.js >=20.19 is required because Rust GUI tests exercise the Playwright web replay adapter.\n' >&2
  printf 'Install or activate a supported Node.js version, then rerun ci/local_check.sh.\n' >&2
  exit 1
}

check_linux_native_deps() {
  if [ "$(uname -s)" != "Linux" ]; then
    return
  fi
  if ! command -v pkg-config >/dev/null 2>&1; then
    printf 'pkg-config is required for Linux native automation dependencies.\n' >&2
    printf 'On Ubuntu, install: sudo apt-get install pkg-config libwayland-dev libx11-dev libxtst-dev libxdo-dev\n' >&2
    exit 1
  fi
  missing=()
  for package in wayland-client x11 xtst xdo; do
    if ! pkg-config --exists "$package"; then
      missing+=("$package")
    fi
  done
  if [ "${#missing[@]}" -gt 0 ]; then
    printf 'Missing Linux native automation pkg-config packages: %s\n' "${missing[*]}" >&2
    printf 'On Ubuntu, install: sudo apt-get install libwayland-dev libx11-dev libxtst-dev libxdo-dev\n' >&2
    exit 1
  fi
}

locally_packageable_crates() {
  # Before the first release, crates.io cannot resolve workspace-internal
  # dependencies for downstream crates. Full ordered publish dry-runs happen in
  # .github/workflows/publish.yml after each dependency is published.
  for crate in \
    greentic-desktop-core \
    greentic-desktop-config \
    greentic-desktop-llm \
    greentic-desktop-session \
    greentic-desktop-telemetry \
    greentic-desktop-gui-assets \
    greentic-desktop-evidence \
    greentic-desktop-registry
  do
    manifest="$(crate_manifest_path "$crate" || true)"
    if [ -n "$manifest" ] && is_publishable "$manifest"; then
      printf '%s\n' "$crate"
    fi
  done
}

ROOT="$(workspace_root)"
cd "$ROOT"

# shellcheck source=ci/crate_publish_order.sh
source ci/crate_publish_order.sh

header "publish crate order"
validate_publish_crate_order

header "workspace dependency policy"
bash ci/workspace_dependency_policy_check.sh

header "Linux native automation dependencies"
check_linux_native_deps

header "no-mock production check"
bash ci/no_mock_production_check.sh

header "no-handrolled scripting check"
bash ci/no_handrolled_scripting_check.sh

header "cargo fmt"
cargo fmt --all -- --check

header "frontend automation dependencies"
ensure_frontend_node
if command -v npm >/dev/null 2>&1; then
  (
    cd frontend/automate-hub
    npm ci
    npx playwright install chromium
  )
else
  printf 'npm is required because Rust GUI tests exercise the Playwright web replay adapter.\n' >&2
  exit 1
fi

header "cargo clippy"
cargo clippy --all-targets --all-features -- -D warnings

header "cargo test"
cargo test --all-features

header "secret leak guard"
if rg -n \
  --glob 'evidence/**' \
  --glob 'logs/**' \
  --glob '*.log' \
  --glob 'bundle.json' \
  --glob 'outputs.json' \
  --glob 'trace.json' \
  'sk-test-super-secret|DEEPSEEK_API_KEY=[^[:space:]]+' .; then
  printf 'Known fake secret appeared in generated evidence or log artifacts.\n' >&2
  exit 1
fi

header "cargo build"
cargo build --all-features

header "cargo doc"
cargo doc --no-deps --all-features

header "installer syntax"
sh -n install.sh
if command -v pwsh >/dev/null 2>&1; then
  pwsh -NoProfile -Command '$null = [scriptblock]::Create((Get-Content ./install.ps1 -Raw))'
else
  printf 'pwsh is not available; skipping PowerShell parser check.\n'
fi

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

#!/usr/bin/env bash
set -euo pipefail

PHASE="${1:-all}"
case "$PHASE" in
  all|policy|frontend|rust-lint|rust-test|package) ;;
  *)
    printf 'Usage: %s [all|policy|frontend|rust-lint|rust-test|package]\n' "$0" >&2
    exit 2
    ;;
esac

run_phase() {
  [ "$PHASE" = "all" ] || [ "$PHASE" = "$1" ]
}

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

if run_phase policy; then
  header "publish crate order"
  validate_publish_crate_order

  header "workspace dependency policy"
  bash ci/workspace_dependency_policy_check.sh

  header "no-mock production check"
  bash ci/no_mock_production_check.sh

  header "no-handrolled scripting check"
  bash ci/no_handrolled_scripting_check.sh

  header "cargo fmt"
  cargo fmt --all -- --check

  header "installer syntax"
  sh -n install.sh
  if command -v pwsh >/dev/null 2>&1; then
    pwsh -NoProfile -Command '$null = [scriptblock]::Create((Get-Content ./install.ps1 -Raw))'
  else
    printf 'pwsh is not available; skipping PowerShell parser check.\n'
  fi
fi

if run_phase frontend; then
  header "frontend checks"
  ensure_frontend_node
  if ! command -v npm >/dev/null 2>&1; then
    printf 'npm is required for frontend checks.\n' >&2
    exit 1
  fi
  (
    cd frontend/automate-hub
    npm ci --no-audit --no-fund
    npm audit --audit-level=high
    npm run lint
    npm run build
    test -f dist/index.html
  )
fi

if run_phase rust-lint; then
  header "cargo clippy"
  cargo clippy --all-targets --all-features -- -D warnings
fi

if run_phase rust-test; then
  header "frontend automation dependencies"
  ensure_frontend_node
  if ! command -v npm >/dev/null 2>&1; then
    printf 'npm is required because Rust tests exercise the Playwright web adapter.\n' >&2
    exit 1
  fi
  (
    cd frontend/automate-hub
    if [ ! -d node_modules ]; then
      npm ci --no-audit --no-fund
    fi
    npx playwright install chromium
  )

  if ! cargo nextest --version >/dev/null 2>&1; then
    printf 'cargo-nextest is required. Install it from https://nexte.st/ and rerun.\n' >&2
    exit 1
  fi
  header "cargo nextest"
  cargo nextest run --all-features --profile ci

  header "cargo doctest"
  cargo test --doc --all-features

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

  if [ "${GREENTIC_LIVE_DESKTOP_TESTS:-0}" = "1" ]; then
    header "live desktop validation"
    bash ci/live_desktop_check.sh
  else
    header "live desktop validation"
    printf 'Skipped. Set GREENTIC_LIVE_DESKTOP_TESTS=1 to run real desktop app validation on this machine.\n'
  fi

  header "cargo doc"
  cargo doc --no-deps --all-features
fi

if run_phase package; then
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
fi

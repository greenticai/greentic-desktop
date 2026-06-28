#!/usr/bin/env bash

PUBLISH_CRATE_ORDER=(
  greentic-desktop-core
  greentic-desktop-config
  greentic-desktop-llm
  greentic-desktop-session
  greentic-desktop-telemetry
  greentic-desktop-gui-assets
  greentic-desktop-adapter
  greentic-desktop-workflow
  greentic-desktop-extension
  greentic-distributor-client
  greentic-desktop-recorder
  greentic-desktop-platform
  greentic-desktop-web
  greentic-desktop-windows
  greentic-desktop-java
  greentic-desktop-terminal
  greentic-desktop-vision
  greentic-desktop-macos
  greentic-desktop-linux
  greentic-desktop-evidence
  greentic-desktop-registry
  greentic-desktop-runner-schema
  greentic-desktop-policy
  greentic-desktop-security
  greentic-desktop-replay
  greentic-desktop-mcp
  greentic-desktop-planner
  greentic-desktop-runtime
  greentic-desktop-gui
  greentic-desktop
)

crate_manifest_path() {
  local crate="$1"
  local manifest
  for manifest in crates/*/Cargo.toml; do
    if [ "$(package_name "$manifest")" = "$crate" ]; then
      printf '%s\n' "$manifest"
      return 0
    fi
  done
  return 1
}

workspace_crates() {
  local manifest
  for manifest in crates/*/Cargo.toml; do
    package_name "$manifest"
  done | sort -u
}

publishable_crates() {
  local crate manifest
  for crate in "${PUBLISH_CRATE_ORDER[@]}"; do
    if manifest="$(crate_manifest_path "$crate")" && is_publishable "$manifest"; then
      printf '%s\n' "$crate"
    fi
  done
}

publishable_workspace_crates() {
  local crate manifest
  while IFS= read -r crate; do
    manifest="$(crate_manifest_path "$crate")"
    if is_publishable "$manifest"; then
      printf '%s\n' "$crate"
    fi
  done < <(workspace_crates)
}

normal_workspace_dependencies() {
  sed -n '/^\[dependencies\]/,/^\[/p' "$1" \
    | sed -n 's/^\(greentic-[A-Za-z0-9_-]*\)[[:space:]]*\.workspace[[:space:]]*=.*/\1/p'
}

publish_order_contains() {
  local wanted="$1"
  local crate
  for crate in "${PUBLISH_CRATE_ORDER[@]}"; do
    if [ "$crate" = "$wanted" ]; then
      return 0
    fi
  done
  return 1
}

publish_order_index() {
  local wanted="$1"
  local crate
  local i=0
  for crate in "${PUBLISH_CRATE_ORDER[@]}"; do
    if [ "$crate" = "$wanted" ]; then
      printf '%s\n' "$i"
      return 0
    fi
    i=$((i + 1))
  done
  return 1
}

validate_publish_crate_order() {
  local crate manifest dep dep_manifest dep_index crate_index missing=0

  while IFS= read -r crate; do
    if ! publish_order_contains "$crate"; then
      printf 'Publishable crate %s is missing from PUBLISH_CRATE_ORDER.\n' "$crate" >&2
      missing=1
    fi
  done < <(publishable_workspace_crates)

  for crate in "${PUBLISH_CRATE_ORDER[@]}"; do
    manifest="$(crate_manifest_path "$crate" || true)"
    if [ -z "$manifest" ]; then
      printf 'PUBLISH_CRATE_ORDER references unknown crate %s.\n' "$crate" >&2
      missing=1
      continue
    fi

    if ! is_publishable "$manifest"; then
      printf 'PUBLISH_CRATE_ORDER includes non-publishable crate %s.\n' "$crate" >&2
      missing=1
      continue
    fi

    while IFS= read -r dep; do
      dep_manifest="$(crate_manifest_path "$dep" || true)"
      if [ -z "$dep_manifest" ]; then
        continue
      fi

      if ! is_publishable "$dep_manifest"; then
        printf 'Publishable crate %s depends on non-publishable workspace crate %s.\n' "$crate" "$dep" >&2
        missing=1
        continue
      fi

      if ! publish_order_contains "$dep"; then
        printf 'Publishable dependency %s of %s is missing from PUBLISH_CRATE_ORDER.\n' "$dep" "$crate" >&2
        missing=1
        continue
      fi

      dep_index="$(publish_order_index "$dep")"
      crate_index="$(publish_order_index "$crate")"
      if [ "$dep_index" -ge "$crate_index" ]; then
        printf 'Publish order is invalid: %s must be before %s.\n' "$dep" "$crate" >&2
        missing=1
      fi
    done < <(normal_workspace_dependencies "$manifest")
  done

  if [ "$missing" -ne 0 ]; then
    return 1
  fi
}

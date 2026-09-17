#!/usr/bin/env bash
# Linux AT-SPI live E2E in a container: builds the Meridian Tauri app and runs
# aws_demo_linux_live_replay (and, with --extraction,
# aws_demo_linux_live_result_extraction) against a real WebKitGTK tree on
# Xvfb + dbus + at-spi2-core. Needs Docker, not sudo, and never touches the
# host desktop's accessibility bus.
#
#   MERIDIAN_SRC=../aws-demo-meridian-insurance ci/linux_atspi/run_live_check.sh
#
# Environment:
#   MERIDIAN_SRC    Meridian repository (required)
#   CACHE_DIR       build caches, default ./target/linux-atspi-live
#   DOCKER_DNS      passed as --dns when set (useful where container DNS is broken)
#   SESSION_TYPE    x11 (default) or wayland (exercises the Wayland adapter; set
#                   GREENTIC_LINUX_ATSPI_KEYBOARD=1 for typing under Xvfb)
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MERIDIAN_SRC="${MERIDIAN_SRC:?set MERIDIAN_SRC to the aws-demo-meridian-insurance checkout}"
CACHE_DIR="${CACHE_DIR:-$ROOT/target/linux-atspi-live}"
IMAGE="${GREENTIC_LINUX_ATSPI_IMAGE:-greentic-linux-atspi-env:1}"
SESSION_TYPE="${SESSION_TYPE:-x11}"
EXTRACTION=0
[ "${1:-}" = "--extraction" ] && EXTRACTION=1

dns_args=()
[ -n "${DOCKER_DNS:-}" ] && dns_args=(--dns "$DOCKER_DNS")

mkdir -p "$CACHE_DIR"/{tmp,cargo-home,target-desktop,target-meridian,home,src,meridian}
CACHE_DIR="$(cd "$CACHE_DIR" && pwd)"

if ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
  container="greentic-linux-atspi-prep-$$"
  docker run --name "$container" "${dns_args[@]}" -v "$ROOT/ci/linux_atspi:/scripts:ro" \
    ubuntu:24.04 /scripts/install_env.sh
  docker commit "$container" "$IMAGE" >/dev/null
  docker rm "$container" >/dev/null
fi

# Copies keep the checkouts clean: builds and evidence land in the cache.
rsync -a --delete --exclude target --exclude node_modules --exclude .git --exclude .greentic \
  "$ROOT/" "$CACHE_DIR/src/"
rsync -a --delete --exclude node_modules --exclude src-tauri/target --exclude dist \
  "$MERIDIAN_SRC/" "$CACHE_DIR/meridian/"

docker run --rm "${dns_args[@]}" \
  -e SESSION_TYPE="$SESSION_TYPE" -e EXTRACTION="$EXTRACTION" \
  -e GREENTIC_LINUX_ATSPI_KEYBOARD="${GREENTIC_LINUX_ATSPI_KEYBOARD:-}" \
  -e GREENTIC_LINUX_ATSPI_TRACE=1 \
  -v "$CACHE_DIR:/cache" -v "$ROOT/ci/linux_atspi:/scripts:ro" \
  "$IMAGE" bash -euo pipefail -c '
    source /root/.cargo/env
    export RUSTUP_HOME=/root/.rustup CARGO_HOME=/cache/cargo-home TMPDIR=/cache/tmp HOME=/cache/home
    (cd /cache/meridian && npm ci --no-audit --no-fund && npm run build \
      && cd src-tauri && CARGO_TARGET_DIR=/cache/target-meridian \
         cargo build --release --features tauri/custom-protocol)
    export GREENTIC_MERIDIAN_APP=/cache/target-meridian/release/aws-demo-meridian-insurance
    cd /cache/src
    export CARGO_TARGET_DIR=/cache/target-desktop
    cargo test --locked -p greentic-desktop-gui --no-run
    export XDG_SESSION_TYPE="$SESSION_TYPE" WEBKIT_DISABLE_COMPOSITING_MODE=1
    /scripts/session.sh cargo test --locked -p greentic-desktop-gui aws_demo_linux_live_replay \
      -- --ignored --nocapture --test-threads=1
    if [ "$EXTRACTION" = 1 ]; then
      /scripts/session.sh bash -c "\"\$GREENTIC_MERIDIAN_APP\" >/dev/null 2>&1 & sleep 6
        python3 /scripts/prepare_quote_result.py
        exec cargo test --locked -p greentic-desktop-gui aws_demo_linux_live_result_extraction \
          -- --ignored --nocapture --test-threads=1"
    fi
  '

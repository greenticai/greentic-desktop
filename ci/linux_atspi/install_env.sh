#!/bin/bash
# Runs inside a fresh ubuntu:24.04 container: everything needed to build the
# Meridian Tauri app and greentic-desktop, plus an X11 + AT-SPI session.
set -euxo pipefail
export DEBIAN_FRONTEND=noninteractive
apt-get update
apt-get install -y --no-install-recommends \
  ca-certificates curl git build-essential pkg-config clang libclang-dev libssl-dev \
  libwebkit2gtk-4.1-dev librsvg2-dev libayatana-appindicator3-dev libxdo-dev \
  libdbus-1-dev libegl1-mesa-dev libgbm-dev libpipewire-0.3-dev libwayland-dev \
  libxi-dev libx11-dev libxcb1-dev libxrandr-dev libxtst-dev \
  at-spi2-core dbus dbus-x11 xvfb xauth wmctrl xdotool procps \
  python3 python3-gi gir1.2-atspi-2.0 nodejs npm
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
  | sh -s -- -y --default-toolchain 1.97.0 --profile minimal -c clippy,rustfmt

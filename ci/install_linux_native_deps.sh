#!/usr/bin/env bash
set -euo pipefail

packages=(
  libdbus-1-dev
  libegl1-mesa-dev
  libgbm-dev
  libpipewire-0.3-dev
  libwayland-dev
  libxi-dev
  libx11-dev
  libxcb1-dev
  libxdo-dev
  libxrandr-dev
  libxtst-dev
  pkg-config
  wmctrl
  xdotool
)

if [ "${1:-}" = "--xvfb" ]; then
  packages+=(xvfb)
fi

sudo apt-get update
sudo apt-get install -y "${packages[@]}"

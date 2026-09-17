#!/bin/bash
# Starts Xvfb + a D-Bus session + the AT-SPI bus, then runs "$@" inside it.
set -euo pipefail
export DISPLAY=:99
Xvfb :99 -screen 0 1400x1000x24 -nolisten tcp >/tmp/xvfb.log 2>&1 &
for _ in $(seq 50); do [ -e /tmp/.X11-unix/X99 ] && break; sleep 0.1; done
unset NO_AT_BRIDGE GTK_A11Y
export GTK_MODULES=gail:atk-bridge
exec dbus-run-session -- bash -c '
  /usr/libexec/at-spi-bus-launcher --launch-immediately >/tmp/atspi.log 2>&1 &
  for _ in $(seq 50); do dbus-send --session --print-reply --dest=org.a11y.Bus /org/a11y/bus org.a11y.Bus.GetAddress >/dev/null 2>&1 && break; sleep 0.1; done
  gsettings set org.gnome.desktop.interface toolkit-accessibility true 2>/dev/null || true
  "$@"
' bash "$@"

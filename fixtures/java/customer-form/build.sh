#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"
if ! command -v javac >/dev/null 2>&1; then
  echo "javac not found; install a JDK to run the real Java fixture" >&2
  exit 77
fi

mkdir -p build
javac -d build CustomerForm.java

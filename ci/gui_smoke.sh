#!/usr/bin/env bash
set -euo pipefail

binary="${1:?usage: ci/gui_smoke.sh PATH_TO_GREENTIC_DESKTOP_BINARY}"
log="$(mktemp)"

"${binary}" gui --no-open --bind 127.0.0.1:0 >"${log}" 2>&1 &
pid="$!"

cleanup() {
  kill "${pid}" >/dev/null 2>&1 || true
  wait "${pid}" >/dev/null 2>&1 || true
  rm -f "${log}"
}
trap cleanup EXIT

url=""
for _ in $(seq 1 100); do
  if grep -q "Greentic Automate Hub: http://127.0.0.1:" "${log}"; then
    url="$(sed -n 's/^Greentic Automate Hub: //p' "${log}" | tail -n 1)"
    break
  fi
  if ! kill -0 "${pid}" >/dev/null 2>&1; then
    cat "${log}" >&2
    exit 1
  fi
  sleep 0.1
done

if [ -z "${url}" ]; then
  cat "${log}" >&2
  echo "GUI did not report a startup URL." >&2
  exit 1
fi

curl -fsS "${url}" | grep -q "Greentic"
curl -fsS "${url%/}/api/v1/health" | grep -q '"status":"ok"'

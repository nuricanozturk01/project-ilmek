#!/usr/bin/env bash
# Opens two hostile "probe" applications in the real desktop client (a real WebView window on the
# current display) and verifies that every escape attempt failed. Linux/WebKitGTK only as written.
# It does NOT cover Windows (WebView2), macOS (WKWebView) or Android; those need their own runs.
set -euo pipefail

. "$(dirname "$0")/../lib/community.sh"
root="$lib_root"
client="$bin/ilmek-client"
[ -n "${DISPLAY:-}" ] || fail "a display is required (set DISPLAY); no windowless mode is claimed"

step "build"
(cd "$root" && cargo build -q -p ilmek-cli -p ilmek-client-app)

step "community, host, publisher"
lib_start_community
lib_join host >/dev/null
lib_join publisher >/dev/null
lib_start_host

step "client device"
client_root="$work/client"
mkdir -p "$client_root"
"$ilmek" join "$profile" --device "$client_root/device" >/dev/null 2>&1

step "canary listener on 127.0.0.1:18080"
python3 -I "$root/scripts/e2e/canary.py" 18080 "$work/canary.log" &
pids+=($!)
sleep 0.5

step "publish two probe applications"
for variant in a b; do
  cp -r "$root/examples/isolation-probe/app" "$work/probe-$variant"
  python3 -I - "$work/probe-$variant/app.toml" "$variant" <<'PY'
import re, sys
path, variant = sys.argv[1], sys.argv[2]
text = open(path).read()
text = text.replace('slug = "probe"', f'slug = "probe-{variant}"')
text = re.sub(r'app_id = "[^"]+"', 'app_id = "01a16490-0000-7000-8000-00000000000%s"' % ('a' if variant == 'a' else 'b'), text)
open(path, 'w').write(text)
PY
  lib_publish_static "$work/probe-$variant" "probe-$variant" "nuri/probe-$variant"
done

if [ -n "${PROBE_PAUSE:-}" ]; then
  printf 'work=%s client_root=%s\n' "$work" "$client_root"
  sleep "$PROBE_PAUSE"
fi

run_clients() { # expected-reports address...
  local expected="$1"; shift
  ILMEK_LOG="${ILMEK_LOG:-warn}" ILMEK_DATA_DIR="$client_root" "$client" "$@" 2>"$work/client.log" &
  local pid=$!
  pids+=($pid)
  python3 -I "$root/scripts/e2e/wait_report.py" "$client_root" "$expected" 60 \
    || { tail -40 "$work/client.log" >&2; fail "the probes never reported"; }
  sleep 1
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

step "run both probes at once in one client process"
run_clients 2 ilmek://nuri/probe-a ilmek://nuri/probe-b
python3 -I "$root/scripts/e2e/check_probe.py" "$client_root" "$work/canary.log"
cp -r "$client_root/storage" "$work/first-run-storage"

step "restart the client: web storage written by the first run must be gone"
rm -rf "$client_root/storage"
run_clients 1 ilmek://nuri/probe-a
step "verify"
python3 -I "$root/scripts/e2e/check_probe.py" "$client_root" "$work/canary.log" --single
if grep -qiE "panick|secret-token|BEGIN (RSA|PRIVATE)" "$work/client.log"; then
  fail "client logs contain a panic or a secret"
fi
printf '\nISOLATION PROBE PASSED (Linux/WebKitGTK, one display, loopback network)\n'

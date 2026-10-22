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
host_peer="$(lib_enroll host member,host --server-ip 127.0.0.1)"
pub_peer="$(lib_enroll publisher member,publisher)"
lib_publisher nuri "$pub_peer" "$host_peer"
lib_start_host

step "client device"
client_root="$work/client"
mkdir -p "$client_root"
"$ilmek" community invite --config "$work/registry.toml" --role member 2>/dev/null >"$work/client.token"
"$ilmek" identity enroll --profile "$profile" --fingerprint "$fingerprint" \
  --device "$client_root/device" --name probe-client --token-file "$work/client.token" >/dev/null 2>&1

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

run_probe() { # slug expected-total-reports
  ILMEK_DATA_DIR="$client_root" "$client" "ilmek://nuri/$1" 2>"$work/client-$1.log" &
  local pid=$!
  pids+=($pid)
  python3 -I "$root/scripts/e2e/wait_report.py" "$client_root" "$2" 60 \
    || { cat "$work/client-$1.log" >&2; fail "the $1 probe never reported"; }
  sleep 1
  kill "$pid" 2>/dev/null || true
  wait "$pid" 2>/dev/null || true
}

step "run probe B, then probe A, each in a fresh client process"
run_probe probe-b 1
run_probe probe-a 2

step "verify"
python3 -I "$root/scripts/e2e/check_probe.py" "$client_root" "$work/canary.log"
if grep -qiE "panick|secret-token|BEGIN (RSA|PRIVATE)" "$work"/client-*.log; then
  fail "client logs contain a panic or a secret"
fi
printf '\nISOLATION PROBE PASSED (Linux/WebKitGTK, one display, loopback network)\n'

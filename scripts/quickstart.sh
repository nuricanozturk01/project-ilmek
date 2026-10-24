#!/usr/bin/env bash
# One command to see İlmek working: starts a node (registry + application host), publishes the
# forum and hello examples, and tells you how to reach them from this or another machine.
# Press Ctrl-C to stop everything.
#
#   scripts/quickstart.sh                  # data goes to ./ilmek-demo (kept between runs)
#   ILMEK_HOME=/tmp/x scripts/quickstart.sh
#
# Needs no certificates, tokens or keys. On a second machine of the same network, build İlmek
# and run `ilmek join`: it finds this node by itself.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
export ILMEK_HOME="${ILMEK_HOME:-$PWD/ilmek-demo}"
bin="${CARGO_TARGET_DIR:-$root/target}/debug"
ilmek="$bin/ilmek"
registry_port="${ILMEK_REGISTRY_PORT:-47401}"
host_port="${ILMEK_HOST_PORT:-47402}"
pids=()

trap 'for p in "${pids[@]:-}"; do kill "$p" 2>/dev/null || true; done; wait 2>/dev/null || true' EXIT

(cd "$root" && cargo build -q -p ilmek-cli -p ilmek-example-forum -p ilmek-example-notes)
mkdir -p "$ILMEK_HOME"

"$ilmek" node --registry-port "$registry_port" --host-port "$host_port" 2>"$ILMEK_HOME/node.log" &
pids+=($!)
wait_port() { for _ in $(seq 1 100); do (exec 3<>"/dev/tcp/127.0.0.1/$1") 2>/dev/null && return 0; sleep 0.1; done; return 1; }
wait_port "$registry_port" && wait_port "$host_port" \
  || { echo "the node did not start; see $ILMEK_HOME/node.log" >&2; exit 1; }

deploy() { # SOURCE REF [BACKEND]
  if "$ilmek" app list 2>/dev/null | grep -q "ilmek://$2 "; then return; fi
  "$ilmek" app pack "$1" --out "$ILMEK_HOME/pkg-${2//\//-}" >/dev/null
  "$ilmek" app install "$ILMEK_HOME/pkg-${2//\//-}" >/dev/null
  if [ -n "${3:-}" ]; then "$ilmek" app set-backend "$2" --exe "$3" >/dev/null; fi
  "$ilmek" app start "$2" >/dev/null
}
deploy "$root/examples/forum/app" nuri/forum "$bin/ilmek-forum-backend"
deploy "$root/examples/hello-html/app" nuri/merhaba

for _ in $(seq 1 100); do
  [ "$("$ilmek" app list 2>/dev/null | grep -c 'state=running')" -ge 2 ] && break
  sleep 0.1
done
for ref in nuri/forum nuri/merhaba; do
  for _ in $(seq 1 50); do "$ilmek" app publish "$ref" >/dev/null 2>&1 && break; sleep 0.2; done
done

cat <<MSG

İlmek is running.
Try, in another terminal (same ILMEK_HOME=$ILMEK_HOME):

  $ilmek resolve ilmek://nuri/forum
  $ilmek call ilmek://nuri/forum forum.topics.create --write --body '{"title":"Merhaba","body":"ilk"}'
  $ilmek call ilmek://nuri/forum forum.topics.list

From another machine on this network:  ilmek join   (finds this node by itself)
                                       ilmek fetch ilmek://nuri/forum
Ctrl-C stops everything. Log: $ILMEK_HOME/node.log
MSG
wait

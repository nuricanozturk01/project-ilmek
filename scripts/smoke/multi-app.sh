#!/usr/bin/env bash
# A08, A09, A13, A14 on one machine: HOST-1 serves three applications at once (two with
# backends, one static); HOST-2 is a separate host process with its own publisher, its own
# application with its own data. Then one backend is stopped, then HOST-1 is killed, and the
# rest must keep working. Separate OS processes on loopback; NOT a Wi-Fi or multi-machine test.
set -euo pipefail

. "$(dirname "$0")/../lib/community.sh"
root="$lib_root"

step "build"
(cd "$root" && cargo build -q -p ilmek-cli -p ilmek-example-forum -p ilmek-example-notes)

step "community, HOST-1 (publisher nuri), one member"
lib_start_community
lib_join host >/dev/null
lib_join publisher1 >/dev/null
lib_join alice >/dev/null
lib_start_host
host1_pid="$host_pid"

step "HOST-2 (publisher ece), a separate process with its own data directory"
host2_port=$((hostport + 10))
lib_join host2 >/dev/null
lib_join publisher2 >/dev/null
cat >"$work/host2.toml" <<TOML
data_dir = "$work/host2"
device_dir = "$work/host2-device"
listen = "127.0.0.1:$host2_port"
advertised = "127.0.0.1:$host2_port"
heartbeat_interval_ms = 1000
TOML
"$ilmek" host serve --config "$work/host2.toml" 2>>"$work/host2.log" &
host2_pid=$!; pids+=($host2_pid)
wait_for "host 2" "(exec 3<>/dev/tcp/127.0.0.1/$host2_port)"

deploy() { # CONFIG DEVICE SOURCE NAME [BACKEND_EXE]
  local config="$1" device="$2" source="$3" name="$4" exe="${5:-}"
  "$ilmek" app pack "$source" --out "$work/$name-pkg" >/dev/null 2>&1
  "$ilmek" app install --config "$config" "$work/$name-pkg" >/dev/null
  local ref; ref="$(sed -n 's/^namespace = "\(.*\)"/\1/p' "$source/app.toml")/$(sed -n 's/^slug = "\(.*\)"/\1/p' "$source/app.toml")"
  if [ -n "$exe" ]; then "$ilmek" app set-backend --config "$config" "$ref" --exe "$exe" 2>/dev/null; fi
  "$ilmek" app start --config "$config" "$ref" 2>/dev/null
  "$ilmek" app publish --config "$config" "$ref" --device "$device" >/dev/null 2>&1
  printf 'deployed %s\n' "$ref"
}

step "HOST-1 serves three applications at once"
deploy "$work/host.toml" "$work/publisher1-device" \
  "$root/examples/forum/app" forum "$bin/ilmek-forum-backend"
deploy "$work/host.toml" "$work/publisher1-device" \
  "$root/examples/notes/app" notlar1 "$bin/ilmek-notes-backend"
deploy "$work/host.toml" "$work/publisher1-device" \
  "$root/examples/hello-html/app" merhaba
wait_for "three running apps on host 1" \
  "[ \"\$(\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -c 'state=running')\" -eq 3 ]"
"$ilmek" app list --config "$work/host.toml" 2>/dev/null

step "HOST-2 serves its own applications (notes under ece, plus a static one)"
# The same notes application under another namespace needs its own application id.
"$ilmek" app init --namespace ece --slug notlar "$work/ece-scaffold" >/dev/null 2>&1
ece_app_id="$(sed -n 's/^app_id = "\(.*\)"/\1/p' "$work/ece-scaffold/app.toml")"
mkdir -p "$work/ece-notes"; cp -r "$root/examples/notes/app/." "$work/ece-notes/"
sed -i -e 's/^namespace = "nuri"/namespace = "ece"/' -e "s/^app_id = .*/app_id = \"$ece_app_id\"/" "$work/ece-notes/app.toml"
deploy "$work/host2.toml" "$work/publisher2-device" \
  "$work/ece-notes" notlar2 "$bin/ilmek-notes-backend"
deploy "$work/host2.toml" "$work/publisher2-device" \
  "$root/examples/announcements/app" duyurular
wait_for "two running apps on host 2" \
  "[ \"\$(\"$ilmek\" app list --config \"$work/host2.toml\" 2>/dev/null | grep -c 'state=running')\" -eq 2 ]"

alice() { "$ilmek" "$@" --device "$work/alice-device" 2>/dev/null; }
wait_for "both hosts visible to the registry" \
  "\"$ilmek\" resolve ilmek://nuri/forum --device \"$work/alice-device\" 2>/dev/null | grep -q RecentHeartbeat && \"$ilmek\" resolve ilmek://ece/notlar --device \"$work/alice-device\" 2>/dev/null | grep -q RecentHeartbeat"

step "each application works and keeps its own data"
alice call ilmek://nuri/forum forum.topics.create --write --body '{"title":"HOST-1 forum","body":"x"}' >/dev/null
alice call ilmek://nuri/notlar notes.create --write --body '{"title":"not-1","body":"host 1 notu"}' >/dev/null
alice call ilmek://ece/notlar notes.create --write --body '{"title":"not-2","body":"host 2 notu"}' >/dev/null
alice call ilmek://nuri/notlar notes.list | grep -q "not-1" || fail "host 1 notes lost"
if alice call ilmek://nuri/notlar notes.list | grep -q "not-2"; then fail "host 2 data visible on host 1"; fi
alice call ilmek://ece/notlar notes.list | grep -q "not-2" || fail "host 2 notes lost"
if alice call ilmek://ece/notlar notes.list | grep -q "not-1"; then fail "host 1 data visible on host 2"; fi
alice fetch ilmek://nuri/merhaba --cache "$work/c1" >/dev/null
alice fetch ilmek://ece/duyurular --cache "$work/c2" >/dev/null

step "A13: stopping one backend leaves the other applications working"
"$ilmek" app stop --config "$work/host.toml" nuri/forum 2>/dev/null
wait_for "forum to stop" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep 'forum' | grep -vq 'state=running'"
if alice call ilmek://nuri/forum forum.topics.list >/dev/null; then fail "a stopped application still answered"; fi
alice call ilmek://nuri/notlar notes.list | grep -q "not-1" || fail "notes affected by the forum stopping"
alice fetch ilmek://nuri/merhaba --cache "$work/c1b" >/dev/null || fail "static app affected by the forum stopping"
"$ilmek" app start --config "$work/host.toml" nuri/forum 2>/dev/null
wait_for "forum to return" "alice call ilmek://nuri/forum forum.topics.list | grep -q 'HOST-1 forum'"

step "A14: killing HOST-1 leaves HOST-2's applications reachable"
kill -9 "$host1_pid"; wait "$host1_pid" 2>/dev/null || true
alice call ilmek://ece/notlar notes.list | grep -q "not-2" || fail "host 2 not reachable after host 1 died"
alice fetch ilmek://ece/duyurular --cache "$work/c3" >/dev/null || fail "host 2 static app not reachable"
if alice call ilmek://nuri/notlar notes.list >/dev/null; then fail "a dead host still answered"; fi

step "HOST-1 returns with its applications and data"
"$ilmek" host serve --config "$work/host.toml" 2>>"$work/host.log" &
host1_pid=$!; pids+=($host1_pid)
wait_for "host 1 apps back" \
  "[ \"\$(\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -c 'state=running')\" -eq 3 ]"
wait_for "host 1 data back" "alice call ilmek://nuri/notlar notes.list | grep -q 'not-1'"

printf '\nALL STEPS PASSED (A08/A09/A13/A14 loopback multi-process)\n'

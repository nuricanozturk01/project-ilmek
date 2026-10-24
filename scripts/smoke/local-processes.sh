#!/usr/bin/env bash
# Starts a registry, a host and three devices as SEPARATE OS processes on loopback, then runs the
# publish -> resolve -> download -> call -> restart flow through the `ilmek` CLI.
# This is a multi-process loopback test. It is NOT a Wi-Fi or multi-machine test.
set -euo pipefail

. "$(dirname "$0")/../lib/community.sh"
root="$lib_root"

step "build"
(cd "$root" && cargo build -q -p ilmek-cli -p ilmek-example-forum -p ilmek-example-notes)

step "registry serve (creates the community on its first run)"
lib_start_community

step "join: host, publisher and two members"
host_peer="$(lib_join host)"
pub_peer="$(lib_join publisher)"
lib_join alice >/dev/null
lib_join bob >/dev/null
printf 'host=%s publisher=%s\n' "$host_peer" "$pub_peer"

step "host serve"
lib_start_host

step "pack, install, configure and start the forum"
"$ilmek" app pack "$root/examples/forum/app" --out "$work/forum-pkg" 2>/dev/null >/dev/null
"$ilmek" app install --config "$work/host.toml" "$work/forum-pkg" 2>/dev/null
"$ilmek" app set-backend --config "$work/host.toml" nuri/forum --exe "$bin/ilmek-forum-backend" 2>/dev/null
"$ilmek" app start --config "$work/host.toml" nuri/forum 2>/dev/null
wait_for "forum backend" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -q 'state=running'"
"$ilmek" app list --config "$work/host.toml" 2>/dev/null

step "publish in the registry"
wait_for "host registration" "grep -q . '$work/host/host.db'"
"$ilmek" app publish --config "$work/host.toml" nuri/forum --device "$work/publisher-device" 2>/dev/null

step "alice and bob resolve, fetch and use the forum"
"$ilmek" resolve ilmek://nuri/forum --device "$work/alice-device" 2>/dev/null
"$ilmek" fetch ilmek://nuri/forum --device "$work/alice-device" --cache "$work/alice-cache" 2>/dev/null
[ -f "$(find "$work/alice-cache" -name index.html | head -1)" ] || fail "package was not downloaded"
created="$("$ilmek" call ilmek://nuri/forum forum.topics.create --write \
  --body '{"title":"Merhaba","body":"İlk konu"}' --device "$work/alice-device" 2>/dev/null)"
echo "created: $created"
topic_id="$(printf '%s' "$created" | sed -E 's/.*"topic_id":([0-9]+).*/\1/')"
"$ilmek" call ilmek://nuri/forum forum.comments.add --write \
  --body "{\"topic_id\":$topic_id,\"body\":\"bob'dan yorum\"}" --device "$work/bob-device" 2>/dev/null
seen="$("$ilmek" call ilmek://nuri/forum forum.topic.get --body "{\"topic_id\":$topic_id}" --device "$work/alice-device" 2>/dev/null)"
echo "$seen" | grep -q "bob'dan yorum" || fail "alice does not see bob's comment: $seen"

step "diagnose"
"$ilmek" diagnose ilmek://nuri/forum --device "$work/bob-device" 2>/dev/null

step "restart the host: data must persist"
kill -TERM "$host_pid"; wait "$host_pid" 2>/dev/null || true
"$ilmek" host serve --config "$work/host.toml" 2>>"$work/host.log" &
host_pid=$!; pids+=($host_pid)
wait_for "host after restart" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -q 'state=running'"
wait_for "the registry to see the new lease" "\"$ilmek\" resolve ilmek://nuri/forum --device \"$work/alice-device\" 2>/dev/null | grep -q RecentHeartbeat"
again="$("$ilmek" call ilmek://nuri/forum forum.topic.get --body "{\"topic_id\":$topic_id}" --device "$work/alice-device" 2>/dev/null)"
echo "$again" | grep -q "bob'dan yorum" || fail "data lost across the restart"

step "kill the backend process: the supervisor restarts it"
backend_pid="$("$ilmek" app list --config "$work/host.toml" 2>/dev/null | sed -E 's/.* pid=([0-9]+).*/\1/')"
kill -9 "$backend_pid"
wait_for "backend restart" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -E 'state=running' | grep -vq \"pid=$backend_pid \""
"$ilmek" call ilmek://nuri/forum forum.topics.list --device "$work/alice-device" 2>/dev/null | grep -q Merhaba \
  || fail "forum data not available after the backend restarted"

lib_join intruder >/dev/null
step "another device cannot take over the namespace"
if "$ilmek" app publish --config "$work/host.toml" nuri/forum --device "$work/intruder-device" >/dev/null 2>&1; then
  fail "a second device published into someone else's namespace"
fi

printf '\nALL STEPS PASSED (multi-process, loopback)\n'

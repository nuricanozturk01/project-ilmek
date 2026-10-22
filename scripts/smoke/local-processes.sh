#!/usr/bin/env bash
# Starts a registry, a host and three devices as SEPARATE OS processes on loopback, then runs the
# publish -> resolve -> download -> call -> restart flow through the `ilmek` CLI.
# This is a multi-process loopback test. It is NOT a Wi-Fi or multi-machine test.
set -euo pipefail

. "$(dirname "$0")/../lib/community.sh"
root="$lib_root"

step "build"
(cd "$root" && cargo build -q -p ilmek-cli -p ilmek-example-forum -p ilmek-example-notes)

step "community init + registry serve"
lib_start_community

step "enroll host, publisher and two members"
host_peer="$(lib_enroll host member,host --server-ip 127.0.0.1)"
pub_peer="$(lib_enroll publisher member,publisher)"
lib_enroll alice member >/dev/null
lib_enroll bob member >/dev/null
printf 'host=%s publisher=%s\n' "$host_peer" "$pub_peer"

step "reuse of a consumed token is refused"
if "$ilmek" identity enroll --profile "$profile" --fingerprint "$fingerprint" \
     --device "$work/reuse-device" --token-file "$work/alice.token" >/dev/null 2>&1; then
  fail "a consumed token was accepted"
fi

step "wrong CA fingerprint is refused"
if "$ilmek" identity enroll --profile "$profile" --fingerprint "$(printf '0%.0s' {1..64})" \
     --device "$work/evil-device" --token-file "$work/alice.token" >/dev/null 2>&1; then
  fail "a profile with the wrong fingerprint was accepted"
fi

step "publisher namespace, key and host grant"
lib_publisher nuri "$pub_peer" "$host_peer"

step "host serve"
lib_start_host

step "pack, install, configure and start the forum"
"$ilmek" app pack "$root/examples/forum/app" --key "$work/publisher.key" --out "$work/forum-pkg" \
  --publisher-id "$publisher_id" 2>/dev/null >/dev/null
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

step "revoked member is refused"
"$ilmek" community revoke-peer --config "$work/registry.toml" --peer "$("$ilmek" community peers --config "$work/registry.toml" 2>/dev/null | grep ' bob ' | awk '{print $1}')" 2>/dev/null
sleep 3
if "$ilmek" resolve ilmek://nuri/forum --device "$work/bob-device" >/dev/null 2>&1; then
  fail "a revoked device could still use the registry"
fi

printf '\nALL STEPS PASSED (multi-process, loopback)\n'

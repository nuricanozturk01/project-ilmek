#!/usr/bin/env bash
# A22: back up a LIVE registry and host (forum with real data), destroy their data directories,
# restore from the backups, restart, and check the state is the one at backup time.
# Separate OS processes on loopback; NOT a Wi-Fi or multi-machine test.
set -euo pipefail

. "$(dirname "$0")/../lib/community.sh"
root="$lib_root"

step "build"
(cd "$root" && cargo build -q -p ilmek-cli -p ilmek-example-forum)

step "community, host, forum with data"
lib_start_community
registry_pid="${pids[0]}"
lib_join host >/dev/null
lib_join publisher >/dev/null
lib_join alice >/dev/null
lib_start_host
"$ilmek" app pack "$root/examples/forum/app" --out "$work/forum-pkg" >/dev/null 2>&1
"$ilmek" app install --config "$work/host.toml" "$work/forum-pkg" 2>/dev/null
"$ilmek" app set-backend --config "$work/host.toml" nuri/forum --exe "$bin/ilmek-forum-backend" 2>/dev/null
"$ilmek" app start --config "$work/host.toml" nuri/forum 2>/dev/null
wait_for "forum backend" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -q 'state=running'"
"$ilmek" app publish --config "$work/host.toml" nuri/forum --device "$work/publisher-device" >/dev/null 2>&1
call() { "$ilmek" call ilmek://nuri/forum "$@" --device "$work/alice-device" 2>/dev/null; }
call forum.topics.create --write --body '{"title":"Yedekte olmali","body":"ilk"}' >/dev/null
call forum.topics.list | grep -q "Yedekte olmali" || fail "setup: topic missing"

step "backup the running registry and host"
"$ilmek" backup registry --config "$work/registry.toml" --out "$work/registry-backup"
"$ilmek" backup host --config "$work/host.toml" --out "$work/host-backup"
"$ilmek" backup verify --dir "$work/registry-backup"
"$ilmek" backup verify --dir "$work/host-backup"
if find "$work/registry-backup" "$work/host-backup" \( -name '*-wal' -o -name '*-shm' \) | grep -q .; then
  fail "a backup contains SQLite side files"
fi
if find "$work/host-backup" -path '*/logs/*' | grep -q .; then fail "a backup contains logs"; fi

step "change after the backup (must NOT survive the restore)"
call forum.topics.create --write --body '{"title":"Yedekten sonra","body":"ikinci"}' >/dev/null
call forum.topics.list | grep -q "Yedekten sonra" || fail "setup: second topic missing"

step "tampered backup is refused, occupied destination is refused"
cp -r "$work/host-backup" "$work/tampered"
victim="$(find "$work/tampered" -name 'host.db' | head -1)"
printf 'X' | dd of="$victim" bs=1 seek=100 conv=notrunc status=none
if "$ilmek" backup verify --dir "$work/tampered" >/dev/null 2>&1; then fail "tampered backup verified"; fi
if "$ilmek" backup restore-host --config "$work/host.toml" --from "$work/host-backup" >/dev/null 2>&1; then
  fail "restore into the live (non-empty) host directory was accepted"
fi

step "disaster: stop everything and destroy both data directories"
kill -TERM "$host_pid" "$registry_pid"; wait "$host_pid" "$registry_pid" 2>/dev/null || true
rm -rf "$work/registry" "$work/host"
if [ -e "$work/registry" ] || [ -e "$work/host" ]; then fail "data directories still exist"; fi

step "restore"
"$ilmek" backup restore-registry --config "$work/registry.toml" --from "$work/registry-backup"
"$ilmek" backup restore-host --config "$work/host.toml" --from "$work/host-backup"

step "restart registry and host from the restored data"
"$ilmek" registry serve --config "$work/registry.toml" 2>>"$work/registry.log" &
pids+=($!)
wait_for "restored registry" "(exec 3<>/dev/tcp/127.0.0.1/$control)"
"$ilmek" host serve --config "$work/host.toml" 2>>"$work/host.log" &
host_pid=$!; pids+=($host_pid)
wait_for "restored host" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -q 'state=running'"
wait_for "the registry to see the restored host" \
  "\"$ilmek\" resolve ilmek://nuri/forum --device \"$work/alice-device\" 2>/dev/null | grep -q RecentHeartbeat"

step "state equals the backup point"
wait_for "the restored forum to answer" "call forum.topics.list"
topics="$(call forum.topics.list)"
echo "$topics" | grep -q "Yedekte olmali" || fail "backed-up topic is gone: $topics"
if echo "$topics" | grep -q "Yedekten sonra"; then fail "post-backup topic survived the restore: $topics"; fi
call forum.topics.create --write --body '{"title":"Geri yuklemeden sonra","body":"yazilabiliyor"}' >/dev/null \
  || fail "restored application is not writable"

printf '\nALL STEPS PASSED (A22 backup/restore, multi-process, loopback)\n'

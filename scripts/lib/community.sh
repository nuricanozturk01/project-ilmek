#!/usr/bin/env bash
# Shared helpers for the loopback multi-process scripts. Source this file; do not execute it.
# Everything runs as separate OS processes on 127.0.0.1. It is NOT a Wi-Fi or multi-machine test.

lib_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
bin="${CARGO_TARGET_DIR:-$lib_root/target}/debug"
ilmek="$bin/ilmek"
pids=()

step() { printf '\n== %s\n' "$*"; }
fail() { printf 'FAILED: %s\n' "$*" >&2; exit 1; }

cleanup() {
  for pid in "${pids[@]:-}"; do kill "$pid" 2>/dev/null || true; done
  wait 2>/dev/null || true
  [ -n "${work:-}" ] && rm -rf "$work"
}

# Poll a command string until it succeeds or 30 seconds pass.
wait_for() {
  local what="$1"; shift
  for _ in $(seq 1 150); do
    if eval "$@" >/dev/null 2>&1; then return 0; fi
    sleep 0.2
  done
  fail "timed out waiting for $what"
}

# lib_start_community: starts the registry (it creates the community on its first run).
# Sets $work, $profile, $control, $hostport.
lib_start_community() {
  work="$(mktemp -d)"
  trap cleanup EXIT
  control=$((20000 + RANDOM % 20000)); hostport=$((control + 2))
  # A private discovery port, so a real node on this network is never answered by mistake.
  export ILMEK_DISCOVERY_PORT=$((40000 + RANDOM % 20000))

  cat >"$work/registry.toml" <<TOML
data_dir = "$work/registry"
listen = "127.0.0.1:$control"
allowed_endpoints = ["127.0.0.0/8"]
TOML
  cat >"$work/host.toml" <<TOML
data_dir = "$work/host"
device_dir = "$work/host-device"
listen = "127.0.0.1:$hostport"
advertised = "127.0.0.1:$hostport"
heartbeat_interval_ms = 1000
TOML
  profile="$work/registry/community.json"

  "$ilmek" registry serve --config "$work/registry.toml" 2>"$work/registry.log" &
  pids+=($!)
  wait_for "registry" "(exec 3<>/dev/tcp/127.0.0.1/$control)"
}

# lib_join NAME -> prints the new device id; state stored in $work/NAME-device
lib_join() {
  local name="$1"
  "$ilmek" join "$profile" --device "$work/$name-device" 2>/dev/null | awk '{print $2}'
}

lib_start_host() {
  "$ilmek" host serve --config "$work/host.toml" 2>>"$work/host.log" &
  host_pid=$!; pids+=($host_pid)
  wait_for "host" "(exec 3<>/dev/tcp/127.0.0.1/$hostport)"
}

# lib_publish_static SOURCE_DIR NAME ADDRESS_SLUG -> packs, installs, starts and publishes
# (as the device in $work/publisher-device)
lib_publish_static() {
  local source="$1" name="$2"
  "$ilmek" app pack "$source" --out "$work/$name-pkg" >/dev/null 2>&1
  "$ilmek" app install --config "$work/host.toml" "$work/$name-pkg" >/dev/null 2>&1
  "$ilmek" app start --config "$work/host.toml" "$3" 2>/dev/null
  wait_for "$name to run" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -q 'state=running'"
  "$ilmek" app publish --config "$work/host.toml" "$3" --device "$work/publisher-device" >/dev/null 2>&1
}

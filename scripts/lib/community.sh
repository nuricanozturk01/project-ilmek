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

# lib_start_community: creates the community and starts the registry. Sets $work, $profile,
# $fingerprint, $control, $hostport.
lib_start_community() {
  work="$(mktemp -d)"
  trap cleanup EXIT
  control=$((20000 + RANDOM % 20000)); enroll_port=$((control + 1)); hostport=$((control + 2))

  cat >"$work/registry.toml" <<TOML
data_dir = "$work/registry"
control_listen = "127.0.0.1:$control"
enroll_listen = "127.0.0.1:$enroll_port"
allowed_endpoints = ["127.0.0.0/8"]
TOML
  cat >"$work/host.toml" <<TOML
data_dir = "$work/host"
device_dir = "$work/host-device"
listen = "127.0.0.1:$hostport"
advertised = "127.0.0.1:$hostport"
trust_refresh_ms = 1000
heartbeat_interval_ms = 1000
TOML

  fingerprint="$("$ilmek" community init --config "$work/registry.toml" --advertise-ip 127.0.0.1 2>/dev/null)"
  [ ${#fingerprint} -eq 64 ] || fail "unexpected fingerprint: $fingerprint"
  profile="$work/registry/community.json"

  "$ilmek" registry serve --config "$work/registry.toml" 2>"$work/registry.log" &
  pids+=($!)
  wait_for "registry" "(exec 3<>/dev/tcp/127.0.0.1/$control)"
}

# lib_enroll NAME ROLES [invite args...] -> prints the new peer id; identity stored in $work/NAME-device
lib_enroll() {
  local name="$1" roles="$2"; shift 2
  "$ilmek" community invite --config "$work/registry.toml" --role "$roles" "$@" 2>/dev/null >"$work/$name.token"
  "$ilmek" identity enroll --profile "$profile" --fingerprint "$fingerprint" \
    --device "$work/$name-device" --name "$name" --token-file "$work/$name.token" 2>/dev/null | awk '{print $2}'
}

# lib_publisher NAMESPACE PUBLISHER_PEER HOST_PEER -> sets $publisher_id; key in $work/publisher.key
lib_publisher() {
  local namespace="$1" publisher_peer="$2" host_peer="$3" pubkey
  pubkey="$("$ilmek" publisher keygen --out "$work/publisher.key" 2>/dev/null)"
  publisher_id="$("$ilmek" community add-publisher --config "$work/registry.toml" --name "$namespace" 2>/dev/null)"
  "$ilmek" community add-publisher-device --config "$work/registry.toml" --publisher "$publisher_id" --peer "$publisher_peer" 2>/dev/null
  "$ilmek" community add-publisher-key --config "$work/registry.toml" --publisher "$publisher_id" --key "$pubkey" >/dev/null 2>&1
  "$ilmek" community grant-namespace --config "$work/registry.toml" --publisher "$publisher_id" --namespace "$namespace" 2>/dev/null
  "$ilmek" community grant-host --config "$work/registry.toml" --publisher "$publisher_id" --host "$host_peer" 2>/dev/null
  "$ilmek" host trust-publisher --config "$work/host.toml" --key "$pubkey" 2>/dev/null
}

lib_start_host() {
  "$ilmek" host serve --config "$work/host.toml" 2>>"$work/host.log" &
  host_pid=$!; pids+=($host_pid)
  wait_for "host" "(exec 3<>/dev/tcp/127.0.0.1/$hostport)"
}

# lib_publish_static SOURCE_DIR NAME ADDRESS_SLUG -> packs, installs, starts and publishes
lib_publish_static() {
  local source="$1" name="$2"
  "$ilmek" app pack "$source" --key "$work/publisher.key" --out "$work/$name-pkg" --publisher-id "$publisher_id" >/dev/null 2>&1
  "$ilmek" app install --config "$work/host.toml" "$work/$name-pkg" >/dev/null 2>&1
  "$ilmek" app start --config "$work/host.toml" "$3" 2>/dev/null
  wait_for "$name to run" "\"$ilmek\" app list --config \"$work/host.toml\" 2>/dev/null | grep -q 'state=running'"
  "$ilmek" app publish --config "$work/host.toml" "$3" --device "$work/publisher-device" >/dev/null 2>&1
}

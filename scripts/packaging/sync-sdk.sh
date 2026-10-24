#!/usr/bin/env bash
# Rebuilds @ilmek/sdk and copies the page-side bundle into the Rust runtime, which embeds it.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
(cd "$root/packages/sdk-js" && bun run build)
cp "$root/packages/sdk-js/dist/ilmek-sdk.iife.js" "$root/crates/ilmek-runtime/assets/ilmek-sdk.iife.js"
echo "SDK bundle synced."

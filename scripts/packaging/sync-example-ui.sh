#!/usr/bin/env bash
# Copies the shared example stylesheet and helpers into every example application's assets.
set -euo pipefail
root="$(cd "$(dirname "$0")/../.." && pwd)"
for app in forum notes hello-html announcements; do
  cp "$root/examples/_ui/ilmek-ui.css" "$root/examples/$app/app/assets/ilmek-ui.css"
done
for app in forum notes; do
  cp "$root/examples/_ui/ilmek-ui.js" "$root/examples/$app/app/assets/ilmek-ui.js"
done
echo "Example UI synced."

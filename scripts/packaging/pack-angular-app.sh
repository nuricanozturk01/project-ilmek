#!/usr/bin/env bash
set -euo pipefail

if [ "$#" -ne 3 ]; then
  echo "usage: $0 <angular-project-dir> <ilmek-app-dir> <output-dir>" >&2
  echo "  <ilmek-app-dir> must contain app/app.toml; <output-dir> must not exist" >&2
  exit 2
fi

ng_project="$1"
app_root="$2"
out="$3"
app_src="$app_root/app"
assets="$app_src/assets"

if [ ! -f "$app_src/app.toml" ]; then
  echo "error: missing $app_src/app.toml" >&2
  exit 1
fi
if [ -e "$out" ]; then
  echo "error: output already exists: $out" >&2
  exit 1
fi
if [ -L "$assets" ]; then
  echo "error: refusing to clear symlinked assets directory: $assets" >&2
  exit 1
fi

(cd "$ng_project" && ng build --configuration production)

shopt -s nullglob
browsers=("$ng_project"/dist/*/browser)
shopt -u nullglob
if [ "${#browsers[@]}" -ne 1 ]; then
  echo "error: expected one dist/<project>/browser, found ${#browsers[@]}" >&2
  echo "  hint: remove stale dist/ folders from older builds and rebuild" >&2
  exit 1
fi
browser="${browsers[0]}"

if [ ! -f "$browser/index.html" ] || [ -L "$browser/index.html" ]; then
  echo "error: $browser/index.html is missing or not a regular file; assets left unchanged" >&2
  exit 1
fi

mkdir -p "$assets"
find "$assets" -mindepth 1 -delete
(cd "$browser" && find . -type f ! -name '*.map' -print0) | while IFS= read -r -d '' file; do
  mkdir -p "$assets/$(dirname "$file")"
  cp "$browser/$file" "$assets/$file"
done

ilmek app pack "$app_src" --out "$out"
echo "Packed $out"

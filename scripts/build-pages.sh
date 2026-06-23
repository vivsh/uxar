#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-"$ROOT/target/pages"}"

rm -rf "$OUT"
mkdir -p "$OUT/public"

cp -R "$ROOT/vyuh/web/landing/." "$OUT/"
cp -R "$ROOT/vyuh/web/public/." "$OUT/public/"

# Rewrite source-local paths for the hosted artifact.
perl -0pi -e 's#\.\./public/#./public/#g; s#\.\./\.\./\.\./docs/book/book/index\.html#./docs/#g' "$OUT/index.html"

mdbook build "$ROOT/docs/book" --dest-dir "$OUT/docs"
touch "$OUT/.nojekyll"

#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="${1:-"$ROOT/target/pages"}"
CSS_MANIFEST="$ROOT/vyuh/web/public/css/manifest.json"

npm --prefix "$ROOT/vyuh/web" run build:css
CSS_FILE="$(node -e "const m=require(process.argv[1]); console.log(m['vyuh.css'] || 'vyuh.css')" "$CSS_MANIFEST")"

rm -rf "$OUT"
mkdir -p "$OUT/public"

cp -R "$ROOT/vyuh/web/landing/." "$OUT/"
cp -R "$ROOT/vyuh/web/public/." "$OUT/public/"

# Rewrite source-local paths for the hosted artifact.
perl -0pi -e 's#\.\./public/#./public/#g; s#\.\./\.\./\.\./docs/book/book/index\.html#./docs/#g; s#\.\./\.\./\.\./docs/book/book/philosophy\.html#./docs/philosophy.html#g' "$OUT/index.html"
perl -0pi -e "s#\.\/public\/css\/vyuh\.css#./public/css/$CSS_FILE#g" "$OUT/index.html"

mdbook build "$ROOT/docs/book" --dest-dir "$OUT/docs"

# mdBook renders the sidebar title as plain text. On the hosted book, make it a
# stable path back to the landing page at the Pages root.
find "$OUT/docs" -name '*.html' -print0 |
  xargs -0 perl -0pi -e 's#<h1 class="menu-title">Vyuh</h1>#<h1 class="menu-title"><a class="vyuh-book-home" href="../">Vyuh</a></h1>#g'

find "$OUT/docs" -name '*.html' -print0 |
  xargs -0 perl -0pi -e "s#href=\"theme/vyuh-[^\"]+\.css\"#href=\"../public/css/$CSS_FILE\"#g"

touch "$OUT/.nojekyll"

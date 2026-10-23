#!/usr/bin/env bash
# Assemble the public Nestbox site: landing page, install guide, screenshots and docs.
#
# The result is a self-contained directory that GitHub Pages can serve as-is:
#   index.html  styles.css  guide/  docs/  assets/  CNAME  .nojekyll  robots.txt  sitemap.xml
#
# usage: scripts/site/build_site.sh [output-dir]
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="${1:-$ROOT/dist/site}"
DOMAIN="${SITE_DOMAIN:-nestbox.mmbits.com}"

say() { printf '[site] %s\n' "$*"; }

[ -f "$ROOT/site/index.html" ] || { echo "missing site/index.html" >&2; exit 1; }
[ -f "$ROOT/site/guide/index.html" ] || { echo "missing site/guide/index.html" >&2; exit 1; }

say "assembling $OUT"
rm -rf "$OUT"
mkdir -p "$OUT/guide" "$OUT/assets"

cp "$ROOT/site/index.html" "$ROOT/site/styles.css" "$OUT/"
cp "$ROOT/site/guide/index.html" "$ROOT/site/guide/guide.css" "$OUT/guide/"

cp -r "$ROOT/assets/brand" "$OUT/assets/brand"
if [ -d "$ROOT/site/assets/guide" ]; then
  mkdir -p "$OUT/assets/guide"
  cp "$ROOT"/site/assets/guide/*.png "$OUT/assets/guide/" 2>/dev/null || true
fi

printf '%s\n' "$DOMAIN" > "$OUT/CNAME"
: > "$OUT/.nojekyll"
printf 'User-agent: *\nAllow: /\nSitemap: https://%s/sitemap.xml\n' "$DOMAIN" > "$OUT/robots.txt"

if python3 -c "import mkdocs" >/dev/null 2>&1; then
  say "building docs"
  ( cd "$ROOT" && python3 -m mkdocs build -q -d "$OUT/docs" >/dev/null )
else
  say "mkdocs not installed, skipping docs"
fi

python3 - "$OUT" <<'PY'
import datetime, pathlib, posixpath, re, sys

out = pathlib.Path(sys.argv[1])
domain = (out / "CNAME").read_text().strip()
pages = [out / "index.html", out / "guide" / "index.html"]

missing = []
for page in pages:
    html = page.read_text(encoding="utf-8")
    for ref in re.findall(r'(?:src|href)="([^"]+)"', html):
        if ref.startswith(("http://", "https://", "#", "mailto:", "data:")):
            continue
        raw = ref.split("#")[0].split("?")[0]
        if not raw:
            continue
        page_url = "/" + page.relative_to(out).parent.as_posix().strip(".")
        resolved = posixpath.normpath(posixpath.join(page_url, raw))
        target = (out / resolved.lstrip("/")).resolve()
        ok = target.is_dir() if resolved.endswith("/") or raw.endswith("/") else target.exists()
        if not ok:
            missing.append(f"{page.relative_to(out)} -> {ref}")

urls = ["/", "/guide/"]
for page in sorted(out.glob("docs/**/index.html")):
    urls.append("/" + str(page.parent.relative_to(out)).replace("\\", "/").strip("/") + "/")
urls = sorted(set(urls))
today = datetime.date.today().isoformat()
(out / "sitemap.xml").write_text(
    '<?xml version="1.0" encoding="UTF-8"?>\n'
    '<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n'
    + "".join(
        f"  <url><loc>https://{domain}{u}</loc><lastmod>{today}</lastmod></url>\n" for u in urls
    )
    + "</urlset>\n",
    encoding="utf-8",
)

shots = sorted((out / "assets" / "guide").glob("*.png"))
print(f"[site] pages: {len(pages)}  docs pages: {len(urls) - 2}  screenshots: {len(shots)}")
for shot in shots:
    print(f"[site]   {shot.name}  {shot.stat().st_size // 1024} KiB")
if missing:
    print("[site] broken local references:")
    for item in missing:
        print("  " + item)
    sys.exit(1)
print("[site] all local references resolve")
PY

say "artifact: $OUT ($(du -sh "$OUT" | cut -f1))"
say "preview: python3 -m http.server 8099 --directory $OUT"

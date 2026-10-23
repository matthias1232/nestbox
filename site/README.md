# Nestbox landing page

The static marketing site for Nestbox: one bootable Ubuntu appliance for sandboxed AI agents.
It is a hand-written HTML and CSS page with no build step, no JavaScript, no CDN and no external
fonts. The two brand images come from the repository's shared `assets/brand/` directory.

## Files

| File | Purpose |
|---|---|
| `index.html` | The whole page, in the required section order: hero, what it does, AI firewall, four ways to run, Nemotron, API, footer. |
| `styles.css` | Design tokens, layout, cards, table and responsive rules. |
| `README.md` | This file. |

Nothing else belongs in this folder. The canonical documentation lives in MkDocs, not here.

## Local preview

From the repository root, serve the repository so that `../assets/brand/` resolves:

```bash
python3 -m http.server 8000
```

Then open <http://localhost:8000/site/>. Opening `index.html` directly from disk also works, but a
local server gives the same relative paths as production.

## Publishing

The landing page is published on **GitHub Pages** with the custom domain
**nestbox.mmbits.com**.

1. Repository **Settings → Pages** sets the source to the deployment output described below and
   records `nestbox.mmbits.com` as the custom domain.
2. GitHub Pages requires a `CNAME` file at the root of the published artifact containing exactly:

   ```text
   nestbox.mmbits.com
   ```

   Setting the custom domain in repository settings creates that file. With an Actions-based
   deployment it must be included in the artifact instead. This repository does not yet contain a
   Pages workflow; the site folder is ready for one.
3. DNS points `nestbox.mmbits.com` at GitHub Pages (apex `A`/`AAAA` records, or a `CNAME` for
   `www`), and **Enforce HTTPS** is enabled once the certificate is issued.
4. Every push to `main` that changes this folder or the brand assets republishes the site.

### Publish layout

`index.html` references `../assets/brand/hero-banner.jpg` and `../assets/brand/icon-1024.jpg` so it
works both in the repository and when served from a nested path. A URL at the domain root resolves
`../assets/...` back to `/assets/...`, so the artifact must place the assets there:

```text
_site/
  index.html             # copied from site/index.html
  styles.css             # copied from site/styles.css
  CNAME                  # nestbox.mmbits.com
  assets/
    brand/
      hero-banner.jpg
      icon-1024.jpg
```

In other words: put the contents of this folder at the publish root, and copy `assets/` next to it.
Do not rewrite the image paths. If Pages is instead configured to deploy the repository root
directly, the page is reachable only at `/site/`; the custom-domain root would not serve it.

## Canonical documentation

Documentation is authored in Markdown under `docs/` and built with **MkDocs** (configuration:
`mkdocs.yml`), not in this folder. The landing page links to the published build under `/docs/`.

- Edit docs in `docs/`, not in `index.html`.
- Performance and capability claims must follow `docs/claims.md`.
- If the docs move to a different host or path, update the `/docs/...` links and the "Read the docs"
  button in `index.html`.
- `mkdocs.yml` currently declares the domain root as its `site_url`. If the build is served under
  `/docs/`, change that value to `https://nestbox.mmbits.com/docs/` so canonical links and the
  sitemap stay correct.

## Design and content rules

- Dark-first only, using the palette in `assets/brand/tokens.json`. Components use 12px radii,
  thin 1px borders and restrained glow.
- The type stack starts with `Inter`, then `ui-sans-serif` and the system UI fonts. Nothing is
  loaded from a font CDN.
- All copy is English. Layout must stay readable at 360px wide with no horizontal page scrolling;
  wide code and table content scrolls inside its own container.
- Every image has descriptive alt text, every interactive element has a visible `:focus-visible`
  style, and motion is limited to CSS hover transitions. `prefers-reduced-motion` is respected.
- Keep the NVIDIA relationship exact. The page may state compatibility with "NVIDIA OpenShell",
  "NemoClaw", "OpenClaw" and "Nemotron", but must not include NVIDIA logos, wordmarks or brand
  green. The footer line is required verbatim:

  > Independent community project. Not affiliated with or endorsed by NVIDIA.

- No third-party logos. GitHub is referenced as a text link only.

## Before publishing

- Check the page at 360px, 768px and a desktop width.
- Confirm `../assets/brand/hero-banner.jpg` and `../assets/brand/icon-1024.jpg` exist.
- Check that the disclaimer and "Maintained by Matthias Binder · mmbits.com" are intact.
- Confirm the `/docs/`, `/api/docs/`, `/api/redoc/` and `/api/schema/v1/` links point where you
  intend. The API links open on a running appliance; the docs links open the MkDocs build.

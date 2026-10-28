# Nestbox icons

The icon set for the Nestbox web UI — the interface used to create, run and
manage sandboxed AI agents, their containers, policies and network links.

One flat SVG file per icon, named in kebab-case, no build step, no sprite
generator, no icon font. Drop a file into your markup or copy its contents
inline.

Every icon lives directly in `assets/icons/`, named after the concept it represents
(`sandboxes.svg`, `policies.svg`, `certificate.svg`, …). Listing or copying the
set is a plain directory read; there is no manifest to keep in sync.

## The 24px grid

Every icon is drawn on a **24×24 viewBox**, authored at `width="24" height="24"`
so a file can be dropped in at its natural size and scaled from there:

```html
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" fill="none" viewBox="0 0 24 24">
```

Shapes are built from whole and half units on that grid (3, 3.5, 4, 7.5, 10.5,
12 …). Circles that mark state — `check-circle`, `clock`, `info`, `x-circle` —
all share the same radius (`8.5`) and the same centre (`12,12`), so they line up
pixel for pixel when they swap in place during a status change.

**Padding.** Nothing is drawn outside a 2px inset, i.e. the live area is the
20×20 box from `(2,2)` to `(22,22)`. A stroke of 1.5 straddles its path, so the
outermost ink lands at about 1.25px from the viewBox edge — tight but never
clipped. This is why the icons keep their weight when they sit next to text in a
table row or a button; nothing touches the edge of its own hit area.

The set is designed for **16, 20 and 24 CSS px**. Below 16px the stroke starts to
close up in dense icons (`backups`, `gpu`, `certificate`) — use 16px as the
floor.

## The 1.5px stroke rule

Every stroke is `stroke-width="1.5"` with `stroke-linecap="round"` and
`stroke-linejoin="round"`. That single weight is what makes the set read as one
family: 1.5 on a 24 grid is the visual equivalent of a 1px hairline at 16px and
of a 2px rule at 32px, so it stays legible at both ends.

```html
<g stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
  <rect x="3" y="3" width="7.5" height="7.5" rx="1.8"/>
  ...
</g>
```

Rules that follow from it:

- **No fills**, with one exception: a solid dot where the dot *is* the meaning
  (the `info` dot, the `alert-triangle` dot, the `server` status LEDs). Those use
  `fill="currentColor"` and `stroke="none"` so they stay a clean disc instead of
  a ring.
- Corners on rectangles are rounded (`rx` 1.5–2.5) to match the rounded stroke
  joins; there are no sharp 90° outer corners anywhere in the set.
- Icons that are mostly a circle (`globe`, `database`, `containers`) use the same
  1.5 weight as the ones that are mostly lines (`plus`, `logs`, `activity`), so a
  row of mixed icons keeps an even optical density.

## One file, both themes

The colour is never baked in. Strokes are `stroke="currentColor"` and solid dots
are `fill="currentColor"`, so an icon takes the colour of whatever it sits in:

```css
.nb-icon        { color: var(--text-muted); }
.nb-icon:hover  { color: var(--text); }
.nb-icon.danger { color: var(--danger); }
```

That means **there is no light and no dark variant** — no `-dark.svg` twins, no
theme switch in the build. The same file is correct on a light surface, on a
dark surface, inside a coloured button and in a disabled state, because the
browser resolves `currentColor` at render time.

Two consequences worth knowing:

- `currentColor` only resolves when the SVG is **in the document**. Inline the
  markup, or load a sprite and reference it with `<svg><use href="#…"></svg>`.
  An `<img src="icons/plus.svg">` renders in the document's default text colour
  and cannot be re-coloured from CSS — for `<img>` you would need a CSS `mask`
  instead.
- The files carry no `width`/`height` in CSS and no `color`, so sizing and
  colouring stay entirely with the caller. Set `width`/`height` (or
  `font-size` + `em`) from CSS.

## These are functional UI icons, not artwork

Nestbox icons are *controls*: they label a nav item, a destructive action, a
health state. They have to be crisp at 16px, identical in weight across the set,
and readable in one glance inside a dense table. That calls for deliberate
geometry — a fixed grid, one stroke weight, optically balanced shapes — which is
why every file here is **hand-authored SVG** and not a generated image.

Studio-generated artwork (a model-rendered PNG, an AI illustration, a traced
bitmap) is the wrong tool for this job. It arrives with soft or inconsistent
edges, hundreds of path nodes or an embedded raster, and it cannot inherit
colour. Rasterising it to 24px would blur the stroke and destroy the 1.5px
rhythm the whole set is built on.

So: no embedded raster images, no `<text>` elements, no `<style>` blocks, no
`id`s, no gradients, no filters. Those features either break theming, break
reuse of the same file more than once in a document (duplicate `id`s), or do not
survive the size the icon is actually used at. What is left is stroke geometry in
`currentColor` — themeable, cacheable in the CSS bundle, and free of runtime
cost.

Please keep it that way when adding icons: match the grid, the 2px inset and the
1.5px weight, and hand-draw the shape.

## Usage

Inline, sized by CSS:

```html
<button class="nb-button" aria-label="Restart sandbox">
  <svg class="nb-icon" viewBox="0 0 24 24" fill="none" aria-hidden="true">
    <!-- contents of restart.svg -->
  </svg>
  Restart
</button>
```

```css
.nb-icon { width: 20px; height: 20px; flex: none; }
```

Decorative icons (icon plus visible text) get `aria-hidden="true"`. An icon that
is the only content of a control needs an accessible name on the control itself
(`aria-label` or visually hidden text) — the SVGs contain no `<title>` and no
text.

## The set

46 icons in three groups.

**Navigation (10)** — `dashboard`, `containers`, `sandboxes`, `policies`,
`builds`, `network`, `backups`, `audit`, `settings`, `api`

**Actions (15)** — `play`, `stop`, `restart`, `trash`, `plus`, `edit`, `save`,
`download`, `upload`, `refresh`, `search`, `filter`, `copy`, `external-link`,
`terminal`

**State and objects (21)** — `check-circle`, `alert-triangle`, `x-circle`,
`info`, `clock`, `shield`, `shield-check`, `lock`, `unlock`, `key`,
`certificate`, `server`, `cpu`, `gpu`, `database`, `globe`, `wireguard`, `box`,
`layers`, `activity`, `logs`

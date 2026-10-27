# styles/ — tokens and layout

## Purpose

Design tokens first, layout second, everything else last. A component that needs
a new colour asks for a token; it does not invent a hex value.

## Files

| File | Responsibility |
|---|---|
| `tokens.css` | Colour, spacing, radius, and type scale |
| `app.css` | Reset, shell layout, tables, forms, drawer, graph |
| `providers-status.css` | Provider status, install origin, and the in-use refusal |

A screen-specific file is the exception, not the pattern: it exists where a
feature says something app.css has no word for. Import it from the module that
uses it, not from `main.jsx`.

## Rules

- A colour is defined once. Two shades of "error red" is how a design drifts.
- Density is a token, not a per-table decision: the console is for scanning.
- The graph and the table must survive a narrow viewport: horizontal scroll
  rather than a squeezed layout.

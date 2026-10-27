# styles/ — tokens and layout

## Purpose

Design tokens first, layout second, everything else last. A component that needs
a new colour asks for a token; it does not invent a hex value.

## Files

| File | Responsibility |
|---|---|
| `tokens.css` | Colour, spacing, radius, and type scale |
| `app.css` | Reset, shell layout, tables, forms, drawer, graph |
| `nodes.css` | Node budget bars and the node drawer; imported by `features/nodes/NodesPage.jsx` |

## Rules

- A colour is defined once. Two shades of "error red" is how a design drifts.
- Density is a token, not a per-table decision: the console is for scanning.
- The graph and the table must survive a narrow viewport: horizontal scroll
  rather than a squeezed layout.

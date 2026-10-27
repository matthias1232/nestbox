# src/ — the console

## Purpose

A SolidJS console for the Nestbox control API. It reads and writes through
`api/`, holds screen logic in `features/*/use*.js`, and renders with components
that know nothing about the domain.

## Layers

| Folder | Holds | Must not contain |
|---|---|---|
| `api/` | One file per backend resource | components, state |
| `components/` | Reusable UI | fetching, domain rules |
| `features/` | One folder per screen | cross-feature imports |
| `state/` | Session and environment | screen logic |
| `styles/` | Design tokens and layout | component-specific hacks |

## Rules

1. No `fetch` outside `api/client.js`.
2. No URL string in a component; call an `api/` function.
3. A screen's logic lives in its `use*.js` so it can be tested without a DOM.
4. Loading, empty, and error states are explicit — otherwise "nothing" and "not
   yet" look identical, which is how a broken screen looks healthy.
5. The CSRF token lives in memory only. `localStorage` is not used for
   credentials, because anything in storage is readable by an injected script.

## Adding a screen

1. Create `features/<name>/` and write its `AGENTS.md`.
2. Write `use<Name>.js` with the state and the calls.
3. Write `<Name>Page.jsx` with markup only.
4. Add the route in `App.jsx` and a row to the table in `features/AGENTS.md`.
   The Admin Center is the one view with a real address (`/sandboxes/<id>`), so
   a link to a sandbox survives a reload and can be pasted into a chat; every
   other view stays a signal.

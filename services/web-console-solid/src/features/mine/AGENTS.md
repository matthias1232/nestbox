# features/mine — the end user's own sandboxes

| File | What it does |
|---|---|
| `MySandboxesPage.jsx` | Only the sandboxes this user owns, with open/terminal/desktop/chats/usage, start and stop, and the destinations that were stopped in them — each openable for 15 minutes or for good. |

The page is deliberately narrower than the admin console. It never widens rights: the
API still scopes every call to the caller, and a denial is opened with the same
`allow` / traffic-window endpoints the Traffic tab uses.

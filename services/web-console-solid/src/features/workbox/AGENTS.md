# features/workbox

The end-user view of one sandbox (`/box/<id>`) and the list of the user's own
sandboxes (`/my`): the switches for the agents' sign-in, the proxy mode and the
evaluation windows, plus live chats, the topology and the shared terminals.

## Files

| File | What it shows |
|---|---|
| `WorkboxPage.jsx` | the whole view for one sandbox: header, cards, panels |
| `MyBoxesPage.jsx` | the sandboxes of the signed-in user |
| `AgentsCard.jsx` | one row per agent: subscription ↔ gateway, sessions, restart |
| `NetworkCard.jsx` | the four proxy modes and the evaluation windows |
| `AllowAllBanner.jsx` | the loud banner while everything is allowed |
| `network.js` | the proxy modes, durations and banner sentence; pure, tested with `node --test network.test.js` |
| `SessionsPanel.jsx` | all shared terminal tabs and the desktop |
| `TerminalPopout.jsx`, `DesktopPopout.jsx` | the pop-out windows without a sidebar |
| `LiveChatsPanel.jsx` | the agents' conversation, live |
| `TopologyPanel.jsx` | the live topology of the sandbox |
| `ModelsCard.jsx` | the models the sandbox may use, with warm-up |
| `MoveDialog.jsx` | preflight and progress of a move to another node |
| `useLive.js` | one subscription to the state stream, filtered per sandbox |

## Rules

This feature may import components from `features/sandboxes/` (`TerminalTab`,
`DesktopTab`, `StackTopology`, `chats.js`). That is an explicit exception to the
one rule in `../AGENTS.md` ("a feature may not import from another feature"),
because a second terminal and a second desktop would be a second truth.

Every panel receives its data as props or loads it through `api/`, never by
reaching into another feature's state.

The vocabulary travels the other way once: `features/sandboxes/PoliciesTab.jsx`
imports the mode labels from `network.js`, so an administrator and a user read
the same words for the same mode. That module is data and pure functions only —
no state, no API call.

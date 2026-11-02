# features/workbox

The end-user view of one sandbox (`/box/<id>`) and the list of the user's own
sandboxes (`/my`): the switches for the agents' sign-in, the proxy mode and the
evaluation windows, plus live chats, the topology and the shared terminals.

## Files

| File | What it shows |
|---|---|
| `WorkboxPage.jsx` | the whole view for one sandbox: header, cards, panels |
| `MyBoxesPage.jsx` | the sandboxes of the signed-in user |
| `useWorkbox.js` | `useWorkbox` (sandbox row + control summary, polling and events) and `useMyBoxes` (the list plus the cheap overview); `splitByOwner` for "Mine" |
| `AgentsCard.jsx` | one row per agent: subscription ↔ gateway, sessions, restart |
| `agents.js` | its pure logic: agent labels, the busy sentence of the switch dialog, the login line |
| `NetworkCard.jsx` | the four proxy modes and the evaluation windows |
| `AllowAllBanner.jsx` | the loud banner while everything is allowed |
| `network.js` | the proxy modes, durations and banner sentence; pure, tested with `node --test network.test.js` |
| `PrivacyCard.jsx` | The DLP findings of this sandbox for its owner: the dashboard and the masked log, collapsed by default |
| `SessionsPanel.jsx` | all shared terminal tabs and the desktop |
| `TerminalPopout.jsx`, `DesktopPopout.jsx` | the pop-out windows without a sidebar |
| `SessionsPanel.jsx` | all shared terminal tabs ("Connected now" from `control.presence`, then terminals and desktop behind two tabs) |
| `sessions.js`, `sessions.test.js` | pure logic of the shared tabs: `mergeTabs` (my list + everybody's, each tab once), `sshCommand`, `puttyHint`; tested with `node --test sessions.test.js` |
| `TerminalPopout.jsx`, `DesktopPopout.jsx` | the pop-out windows without a sidebar: one tab with its viewers, or the desktop, each filling the window |
| `LiveChatsPanel.jsx` | the agents' conversation, live |
| `livechats.js` | turns a recorded call into a turn, groups turns by session, shortens text (pure, tested by `livechats.test.js`) |
| `TopologyPanel.jsx` | the live topology of the sandbox, and the pulse of hosts an event just named |
| `ModelsCard.jsx` | the models the sandbox may use, with warm-up |
| `models.js`, `models.test.js` | the card's logic: readiness badges, scope labels, who may manage |
| `MoveDialog.jsx` | preflight and progress of a move to another node |
| `move.js` | the nine move steps, their state map from events and polls, the node order |
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
`sessions.js` is the reverse of the exception above: `features/sandboxes/` takes
that one module from here, because the tab strip in the Admin Center and in the
Workbox is the same list. It is pure logic — no JSX, no state, no API call — so
it cannot become a second truth. The same holds for `useLive.js`, which a
terminal imports for its presence events so a page keeps one event stream.
| `RequestStreamCard.jsx` | Which models this sandbox uses right now |
| `WorkboxNotice.jsx` | What the end user is told about who may read the chats |
| `LiveEventsPanel.jsx` | Proxy, OpenShell and MCP events of this sandbox, live: one stream, three lists, pause |
| `liveEvents.js`, `liveEvents.test.js` | which list an event belongs to and what a row says (pure, tested with `node --test liveEvents.test.js`) |

## Live events

`LiveEventsPanel` subscribes to `api/telemetry.js`'s `stream` once and keeps a
ring buffer of 200 rows; the proxy, OpenShell and MCP lists are filters over
that one buffer, so switching a segment never loses history. The API has no
per-sandbox telemetry stream, so the panel filters on `sandbox_id` itself.

An OpenShell row is recognised by its mark, not by its product: the control API
writes `listener: "openshell"` on a network row and `resolver: "openshell"` on a
DNS row (`pull_openshell_events` in `services/telemetry_service.rs`). An MCP row
is a network row with `mcp_method`/`mcp_tool` set, which takes precedence — a
tool call made through OpenShell is still a tool call. See `liveEvents.js`.

`TopologyPanel` feeds `pulse` to `StackTopology`: a `Set` of hosts that had an
event in the last two seconds. `StackTopology` adds `.pulse` to the card of such
a host (destination cards are named `host:port`, so the port is cut off), and
`styles/stack-topology.css` fades it 1 → 0.4 → 1 in 600 ms. Without the prop
nothing changes.

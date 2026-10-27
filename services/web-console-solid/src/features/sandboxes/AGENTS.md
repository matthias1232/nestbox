# sandboxes/ — the inventory and the detail drawer

## Purpose

The screen an operator lives in: every sandbox, its live state, and the actions
on it.

## What the page must show

- The lifecycle status the control plane believes, and the runtime facts it
  observed, side by side. They can disagree, and that disagreement is the
  interesting information.
- Address, uptime, and resource limits, because "is it running" is not the same
  as "is it reachable".
- Which node it is on, and — at create time — what that machine has left to
  give.
- The log tail without leaving the page, because a failure is diagnosed from the
  log.

## Rules

- Paging is server-side. The page never asks for everything.
- A lifecycle action is disabled when the current status does not permit it, and
  the reason is the domain rule, not a guess in the component.
- A destructive action asks once, and says what will be destroyed.
- A resource pre-check compares the request against the budget `GET /nodes`
  returned. It never adds up capacity itself: a request above
  `budget.ram_free_bytes` or above `budget.cpu_cores` disables submit and names
  the number, and the server's own `422 node.insufficient_resources` /
  `404 node.not_found` is shown when it answers instead. A node nobody measured
  has no budget to check against, and the form says so rather than treating zero
  as full.
- Removing an agent decides what happens to its persistent home. The "keep its
  data" checkbox is that decision, it defaults to true, and `keep_persistent`
  carries it on the same DELETE.

## Files

| File | Responsibility |
|---|---|
| `useSandboxes.js` | Paging, filters, selection, actions; no JSX |
| `SandboxListPage.jsx` | Table, facets, bulk bar, the New sandbox drawer |
| `CreateSandboxForm.jsx` | Name, pack, node, memory, CPU, PIDs; the node's free numbers beside them (`feat/ui-sandbox-resources`) |
| `useCreateSandbox.js` | Nodes, packs, the draft, and the pre-check; no JSX (`feat/ui-sandbox-resources`) |
| `SandboxDetail.jsx` | Drawer with overview, agents, logs, shell, rules, inference |
| `AgentsTab.jsx` | Agents of one sandbox: list, add, remove with "keep its data" (`feat/ui-sandbox-resources`) |
| `useSandboxAgents.js` | Agents, pack catalog, add and remove; treats `agent.exists` and `agent.not_found` as sentences, not crashes (`feat/ui-sandbox-resources`) |
| `AdminCenter.jsx` | The full-screen Admin Center shell: header, actions, tab strip; owns no tab body |
| `DashboardTab.jsx` | Metrics, top processes, last blocks, MCP servers (`feat/admin-console-shell`) |
| `TrafficTab.jsx` | Live table, filters, "allow this", temporary window (`feat/console-traffic`) |
| `McpTab.jsx` | Servers, tools, calls, visibility per transport (`feat/console-mcp`) |
| `TerminalTab.jsx` | xterm.js over the terminal WebSocket (`feat/console-terminal`) |
| `AccessTab.jsx` | Ports, SSH key, remote snippets (`feat/console-access`) |
| `MapTab.jsx`, `PoliciesTab.jsx` | Live edges, effective rules, deployed state (`feat/console-map-policies`) |
| `ProcessesTab.jsx`, `InventoryTab.jsx`, `LogsTab.jsx` | `docker top`, snapshot, log tail (`feat/console-inventory`) |

## The drawer's Agents tab

The drawer (`SandboxDetail.jsx`) answers the quick questions and owns its own tab
list; the Admin Center's tab list is untouched by this package. "Agents" reads
`GET /sandboxes/{id}/agents`, adds one with `POST …/agents` from the catalog at
`GET /agent-packs`, and removes one with `DELETE …/agents/{instance}` carrying
`keep_persistent`. It also shows the generated entrypoint. After an add the
sentence does not claim the agent is up: a running container keeps the command it
was created with until it is recreated.

## The Admin Center contract

`AdminCenter.jsx` is the shell and belongs to the integrator: header, lifecycle
actions, tab strip, and the fetch of the sandbox row. A tab file belongs to one
package and receives:

- `props.sandbox` — the stored row (name, pack, image, status, runtime facts);
- `props.revision` — bumped by `reload`, for a tab that wants to refetch;
- `props.reload` — re-read the sandbox after an action.

A tab never edits the shell. That is what lets seven console packages run at
once and is checked by review, not by a test: the tab list in `AdminCenter.jsx`
is the one place a tab is registered.

Until its package lands, a tab renders `PendingPanel` and says what belongs
there, which endpoint it will read, and who owns it. An empty box looks broken;
sample data looks finished. Both lie.

## The shell tab

A typed line runs as `bash -lc <line>` through `POST /sandboxes/{id}/exec`. It
is request/response, not a PTY: the output comes back when the command ends,
capped at 64 KiB, with the exit code. That is enough to ask Hermes a question
or inspect a file, and it keeps every run in the audit trail as an argument
vector. An interactive terminal is a later transport, not a change to this one.

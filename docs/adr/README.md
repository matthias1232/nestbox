# Architecture Decision Records

Each file here records one decision: the context, the choice, the alternatives
that were rejected and why, the consequences, and what would invalidate it.

ADRs are immutable once accepted. To change a decision, write a new ADR that
supersedes the old one and note the supersession in both files.

| Number | Title | Status |
|---|---|---|
| [0001](0001-postgresql-as-only-control-store.md) | PostgreSQL as the only control store | Accepted |
| [0002](0002-layered-architecture-and-enforced-boundaries.md) | Layered architecture with enforced boundaries | Accepted |
| [0003](0003-documentation-as-a-build-artifact.md) | Documentation as a build artifact | Accepted |
| [0004](0004-session-auth-rbac-and-tenant-scope.md) | Session auth, RBAC, and tenant scope | Accepted |
| [0005](0005-secrets-as-locators-only.md) | Secrets as locators only | Accepted |
| [0006](0006-cursor-paging-and-facets.md) | Cursor paging and server-side facets | Accepted |
| [0007](0007-policy-scope-mirrors-openshell.md) | Policy scope mirrors OpenShell | Accepted |
| [0008](0008-hot-reloadable-network-section.md) | Only the network section is hot-reloadable | Accepted |
| [0009](0009-event-bus-behind-a-trait.md) | The event bus sits behind a trait | Accepted |
| [0010](0010-telemetry-partitioned-by-month.md) | Telemetry is partitioned by month | Accepted |
| [0011](0011-rule-from-event-is-a-two-step-workflow.md) | Rule from event is a two-step workflow | Accepted |
| [0012](0012-gateway-settings-are-not-policy.md) | Gateway settings are not policy | Accepted |
| [0013](0013-model-catalog-requirements-are-data.md) | Model requirements are data | Accepted |
| [0014](0014-openshell-cli-feature-parity.md) | OpenShell CLI feature parity | Accepted |
| [0015](0015-published-api-contract-is-binding.md) | The published API contract is binding | Accepted |
| [0016](0016-provider-credentials-resolved-at-use.md) | Provider credentials are resolved at use, never stored or returned | Accepted |
| [0017](0017-server-computed-bars-and-honest-unified-memory.md) | Resource bars are server-computed, and unified memory is recognised, never inferred | Accepted |
| [0018](0018-a-model-is-an-install-on-a-node.md) | A model is an install on a node, and the fit is measured against what is free | Accepted |
| [0019](0019-catalog-sync-derives-by-rule.md) | The catalog is derived from the repositories by rule, and a curated row wins | Accepted |
| [0020](0020-packs-are-data.md) | Baselines, agent packs, and tools are data | Accepted |
| [0021](0021-persistence-is-bind-mounts.md) | Persistent data lives in bind mounts, one read-write mount per root | Accepted |
| [0022](0022-egress-is-one-proxy-default-deny-fail-closed.md) | One egress proxy per node, default deny, fail closed | Accepted |
| [0023](0023-terminal-sessions-are-audited-without-content.md) | A terminal session is audited by who and when, never by what | Accepted |
| [0024](0024-ssh-key-is-a-one-time-download.md) | The sandbox SSH key is a one-time download, and the database holds a locator | Accepted |
| [0025](0025-sandbox-metrics-live-in-postgresql.md) | Sandbox metrics live in PostgreSQL, raw for a day and rolled up for a week | Accepted |
| [0026](0026-mcp-calls-are-traffic-and-stdio-is-not-egress.md) | An MCP call over HTTP is traffic; a stdio server is not egress | Accepted |
| [0027](0027-bodies-are-logged-fully-and-the-redactor-is-the-boundary.md) | Bodies are logged fully, and the redactor is the boundary | Accepted |
| [0028](0028-model-serving.md) | An install serves one artefact from one pinned image | Accepted |
| [0029](0029-a-node-is-a-row-plus-an-adapter.md) | A node is a row plus an adapter; multi-node is not a rename | Accepted |
| [0030](0030-sandboxes-that-belong-together-are-linked-explicitly.md) | Sandboxes that belong together are linked explicitly, one level deep | Accepted |
| [0031](0031-a-sandbox-is-reached-through-its-nodes-address.md) | A sandbox is reached through its node's address, never the container's | Accepted |
| [0032](0032-the-stack-is-drawn-from-facts-and-helpers-link-themselves.md) | The stack diagram is arranged from facts, and a lone helper links itself | Accepted |
| [0033](0033-published-ports-are-held-open-by-the-runner.md) | Published ports are held open by the runner, and SSH is answered by it | Accepted |
| [0034](0034-the-proxy-identifies-by-address-and-asks-before-it-tunnels.md) | The proxy identifies a sandbox by its address, and asks before it tunnels | Accepted |
| [0035](0035-a-sandbox-is-administered-live-and-starts-connected.md) | A sandbox is administered live, and its agent starts connected | Accepted |
| [0036](0036-sandboxes-run-on-openshell.md) | Sandboxes run on the NVIDIA OpenShell gateway | Accepted |

## Writing a new ADR

Copy the shape of an existing file. Keep it short. The value is in the
*rejected alternatives* and the *invalidation conditions*, because those are
what a future reader cannot reconstruct from the code.

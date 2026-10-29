# Project Brain

This folder is the project's knowledge for humans and AI agents. It explains
**why** the system is shaped the way it is, in prose.

It is not a duplicate of the code documentation. For *what* a module does, read
the `AGENTS.md` next to the code. For *why a decision was made*, read
`docs/adr/`. This folder connects the two: it describes the product intent and
points at the places that implement it.

## How to use this

| You want to know | Read |
|---|---|
| What the product is and who it serves | `00-index.md` |
| How a request flows from browser to container | `control-plane.md` |
| How policy is modelled and enforced | `policies.md` |
| How the live console gets its data | `telemetry.md` |
| How models are installed and served | `models.md` |
| What goes into a built image, and why a build is verified | `builder.md` |
| How gateways and supervisors relate | `gateways.md` |
| What the security posture is | `security.md` |
| What M1 deliberately does not do | `../architecture/scope.md` |

## Relationship to the infrastructure brain

Infrastructure facts — host names, addresses, access paths, cluster layout —
live in `/home/core/infra-brain/`. This folder does not repeat them; it points
at them. If you need an IP, read the infrastructure brain. If you need to know
why the control plane stores a locator instead of a secret, read here.

## Relationship to the graph

`graphify-out/` holds a machine-readable graph of the repository, regenerated in
CI. Use it to answer structural questions:

```bash
graphify query "where is the tenant scope applied"
graphify explain "policy_resolution"
```

## Keeping this current

When a change alters how the system works rather than how a file is written,
update the relevant document here **and** write an ADR. When a change is purely
local to a module, updating that module's `AGENTS.md` is enough.

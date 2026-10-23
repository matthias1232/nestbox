# Nestbox

One bootable Ubuntu image that turns any machine — or any GPU cloud VM — into a governed
workspace for AI agents.

## Start here

- [Roadmap](roadmap.md) — what exists today and in which order it grows
- [API](api/overview.md) — authentication, errors, streaming
- [AI firewall](policies/network-rules.md) — the policy model in practice
- [Models](models/nemotron.md) and [hardware](hardware/dgx-spark.md) — what runs where
- [Claims](claims.md) — every number with its source

## The idea

Running an agent stack is normally forty commands, six config files and a firewall written by
hand. Nestbox reduces that to one boot, one web UI and one API — while keeping the isolation and
the policy engine that make running agents safe at all.

Django is the control plane. NVIDIA OpenShell is the runtime. The appliance is the packaging.

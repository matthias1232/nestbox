# ADR 0031 — A sandbox is reached through its node's address, never the container's

Status: Accepted
Date: 2026-10-28

## Context

An operator wants to see, per sandbox, how to connect: SSH and one port per agent.
The container has an address on the runtime network, but that address is not
reachable from outside and changes when the container is recreated.

## Decision

Published ports are bound on the **node** (`nodes.public_address`, ADR 0021), and
`GET /sandboxes/{id}/connect` answers with the node's address, its hostname
(`nodes.public_hostname`, display only), every published port with its dial
address, and the SSH key state. The container address is deliberately absent.

The hostname is dialled in preference to the address when both exist, and is never
resolved or bound by the control plane: a wrong hostname misleads, it cannot
misroute.

With no address set, ports are bound to `127.0.0.1` on the node. The answer says
so in a sentence instead of showing a dial address that cannot work, and the
console offers to set the address.

Changing the address does not move ports that are already published; they keep the
binding they were made with. The console says so after saving.

## Alternatives rejected

**Show the container address.** It looks useful and is wrong from every machine
that is not on the node.

**Three client calls (ports, key, node).** The panel could show ports from one
moment and an address from another; one read cannot.

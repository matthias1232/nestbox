# Supervisor agent

`supervisor.py` is the process that runs inside every sandbox built by the
recipe builder, next to the agent the recipe named. It is not an image of its
own: the builder copies this file into the image as
`/usr/local/bin/nestbox-agent-supervisor` and starts it as an extra slot in the
generated supervisor script.

It does two things:

1. **Report.** Every `NESTBOX_SUPERVISOR_INTERVAL` seconds it posts what it
   measured to `POST /api/v1/internal/supervisors/{id}/report`, authenticated
   with the per-sandbox token from `NESTBOX_SUPERVISOR_TOKEN`. The checks are
   model reachability, proxy variables, installed tools, the agent process,
   free memory, and policy drift; each carries the measured value.
2. **Execute a fixed whitelist.** Commands left in
   `~/.nestbox-supervisor/commands/` are applied, with the result written to
   `~/.nestbox-supervisor/results/`. The allowed types are `apply_policy`,
   `restart_agent`, and `run_check`. Anything else is answered `rejected` and
   never run — the control plane validates too, and this is the boundary that
   holds if it is wrong.

`restart_agent` stays refused until `self_healing` is on in the delivered
settings **and** a `restart_command` is configured; there is no default command,
because guessing one is how a supervisor becomes a second administrator.

The token is the sandbox's, not the node's: it opens exactly one report
endpoint for exactly one row. The node token never enters a sandbox.

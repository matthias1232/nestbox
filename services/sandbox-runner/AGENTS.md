# sandbox-runner — the Docker adapter

## Purpose

The only process that holds the Docker socket. The control API never talks to
Docker; it talks to this service over an internal HTTP contract, and this
service turns the request into a container call.

That separation is the security boundary: a compromise of the API does not
yield the host, because the API has no socket. It is also the migration path:
replacing Docker with NVIDIA OpenShell means replacing this adapter, not the
control plane.

## Contract (internal, not published)

| Method | Path | Does |
|---|---|---|
| `GET` | `/healthz` | Docker reachable |
| `GET` | `/v1/sandboxes` | Every managed container with live facts |
| `POST` | `/v1/sandboxes` | Create from an approved image; idempotent on name |
| `GET` | `/v1/sandboxes/{name}` | Live facts for one |
| `POST` | `/v1/sandboxes/{name}/{start,stop,restart,remove}` | Lifecycle |
| `GET` | `/v1/sandboxes/{name}/logs?tail=` | Last lines of stdout/stderr |
| `POST` | `/v1/sandboxes/{name}/exec` | Run one argument vector, return output |
| `GET` | `/v1/hardware` | Raw `/proc` and `nvidia-smi` text for the control plane to parse |
| `POST` | `/v1/builds` | Build a recipe image from a generated Dockerfile |
| `POST` | `/v1/models/download` | Fetch one GGUF into `NESTBOX_MODEL_BASE` |
| `POST` | `/v1/models/serve` | Start one model server container |
| `GET`/`DELETE` | `/v1/models/serve/{key}` | Inspect or stop it |
| `GET` | `/v1/egress` | The sandbox network, the proxy, and the redirect state, as facts |
| `GET` | `/v1/sandboxes/{name}/stats` | One `docker stats` sample |
| `GET` | `/v1/sandboxes/{name}/processes` | `docker top`, capped and typed |
| `GET` | `/v1/sandboxes/{name}/inventory` | A fresh snapshot from the fixed probe |
| `GET` | `/v1/sandboxes/{name}/terminal` | The terminal WebSocket |

## Security rules

1. Only images in `NESTBOX_ALLOWED_IMAGES` can be started. A request for any
   other image is a 422, whatever the API said.
2. Containers run with every capability dropped, `no-new-privileges`, a memory
   limit, a CPU limit, and a PID limit. No privileged mode, no host network, no
   host mounts; the request model has no field for them.
3. A command for `exec` is an argument vector. There is no field that takes a
   shell string, and the adapter never builds one.
4. Exec output is capped (default 64 KiB) and the call has a timeout, so one
   command cannot exhaust the API's memory or hold a worker forever.
5. The runner listens only on the internal `control` network and is not
   published on the host.
6. A model server starts only from an image in `NESTBOX_MODEL_IMAGES`, compared
   as an exact `repo@sha256:…` reference. Every absolute path in its command
   must be the mounted model file, the model directory is mounted read-only at
   `/models`, and the container joins the internal runtime network only — so it
   can answer the API and the sandboxes, and reach nothing else.
7. A model download is written to `<file>.part` and renamed after the last
   byte; the key, repository, and file name are validated and the resolved
   directory must stay inside `NESTBOX_MODEL_BASE`.
8. The sandbox network is created `internal: true`, and a network that exists
   with `internal=false` is refused with 503 instead of reused. See below.
9. The egress proxy is joined to that network by this adapter, under the alias
   the sandbox environment names.

## The sandbox network has no route out

Docker has no per-container "no default route", so the network carries the
property: `internal: true` means the bridge has no gateway and no NAT. Measured
on the appliance, a container on it has a route to its own `/16` and nothing
else — `/proc/net/route` holds one entry with gateway `0.0.0.0`, `wget` to a
public address fails with `Network unreachable`, and an external name answers
`SERVFAIL`. That last point is why the environment the control API writes names
the proxy by hostname: container names resolve, public names do not.

`ensure_network` creates the network and refuses a drift. A network that already
exists with `internal=false` is the state this replaced — every sandbox on it had
a default route and full egress, and nothing was policed while the compose file
said `internal: true` — so a create against it fails instead of quietly
succeeding. `internal` cannot be flipped on a live bridge, so the fix is to
remove the network; the next create recreates it. `/healthz` and `/v1/egress`
report which state you are in.

The proxy is on that network because this adapter puts it there
(`attach_egress_proxy`), with `aliases=["egress-proxy"]`: a plain
`network connect` gives a container only its container name, so without the
alias the sandbox environment would name a host that does not resolve. It is the
only container besides this adapter that belongs on the network; the control API
stays on `control`. The join happens on `create` and again on `start`/`restart`,
because a `docker compose up` that recreates the proxy container drops its
endpoint and an existing sandbox would otherwise come back with a proxy it cannot
reach. The compose file passes this adapter the network name Compose actually
creates (`${COMPOSE_PROJECT_NAME:-nestbox}_runtime`), so the sandbox, model, API,
and proxy containers share one internal bridge; the runtime connect is then a
no-op that only repairs a proxy that Compose recreated.

## Redirect rules, and what they are not

`prepare_redirects` installs `80/443/8080 → 3128` in the proxy container's own
NAT table, idempotently (`-C` before `-A`). The proxy container is the only
component with `NET_ADMIN` (deploy README); a sandbox has every capability
dropped and cannot install anything, which is the point.

The rules are convenience, never the boundary. A sandbox has no default route, so
a raw socket is already unroutable; the redirect only helps a client that
addresses the proxy as a gateway-like host on 80/443/8080 instead of 3128. A
failure to install them is therefore logged and reported, not fatal: a redirect
can only narrow what is reachable, so its absence cannot open a hole. The envoy
image as pinned has no `iptables` binary, so `/v1/egress` currently answers
`unavailable: the proxy image has no iptables` — the honest value, and the
package that owns `services/egress-proxy/Dockerfile` can add the package.

## Why not a PTY

Phase 1 exec is request/response: the command runs, the output returns. A
streaming terminal needs a WebSocket, a resize protocol, and a session that
survives a proxy — a separate transport worth doing properly later, not a
corner cut here.

## Files

| File | Responsibility |
|---|---|
| `app/main.py` | The FastAPI adapter, the sandbox network, the egress wiring, and model serving |
| `app/stats.py` | One-shot `docker stats` samples and the poster loop |
| `app/processes.py` | `docker top` for one sandbox |
| `app/inventory.py` | Runs the probe and parses its sections |
| `app/inventory.sh` | The fixed probe script, executed inside the container |
| `Dockerfile` | Image build |

## Samples, processes, inventory

Three reads of the same container, each with its own cost:

- **A sample is one `docker stats` one-shot**, not a stream. A stream is a socket
  per sandbox that has to survive a runner restart, and the console only needs a
  point every five seconds. The poster thread starts only when
  `NESTBOX_METRICS_POST_URL` is set and posts one batch per tick with
  `NESTBOX_NODE_TOKEN`; a failed batch is dropped rather than queued, because a
  queue of stale points draws a lie.
- **The process table is `docker top`**, which is the list `ps` inside the
  container would show without running a command through the agent's shell. The
  `ps` column set is a constant in `app/processes.py`: `ps` accepts a format
  string, and a caller-supplied one is enough to read a file the caller should
  not see. A stopped container answers 409, not an empty table.
- **The inventory is a script that ships with this image**, piped to `sh` inside
  the container. It takes no argument, so nothing an agent inside the container
  or a caller outside it can do changes what it prints. It prints environment
  variables **by name only** (`cut -d= -f1`), and lists files under the agent's
  home rather than reading them. A partial answer carries a `note` instead of
  looking like a complete one.

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

## Why not a PTY

Phase 1 exec is request/response: the command runs, the output returns. A
streaming terminal needs a WebSocket, a resize protocol, and a session that
survives a proxy — a separate transport worth doing properly later, not a
corner cut here.

## Files

| File | Responsibility |
|---|---|
| `app/main.py` | The FastAPI adapter |
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

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

## Why not a PTY

Phase 1 exec is request/response: the command runs, the output returns. A
streaming terminal needs a WebSocket, a resize protocol, and a session that
survives a proxy — a separate transport worth doing properly later, not a
corner cut here.

## Files

| File | Responsibility |
|---|---|
| `app/main.py` | The FastAPI adapter |
| `Dockerfile` | Image build |

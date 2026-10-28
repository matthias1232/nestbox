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
| `GET` | `/v1/sandboxes/{name}/terminal` | WebSocket: a PTY (`bash`) in the container, resized by a control frame |

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

## The PTY

`terminal.py` is the second transport: `exec_create(tty=True, stdin=True)` plus
`exec_start(socket=True)`, and the socket is pumped in both directions. Exec
stays request/response because that is what a probe and the console's "run" box
need; a shell needs a terminal, and `vim` and `htop` need a window size, which is
what `{"type":"resize","cols":80,"rows":24}` is for.

Three rules hold it together:

1. **Exactly one reader for the PTY, for the life of the session.** A reader per
   loop iteration looks simpler and silently drops bytes: cancelling an
   `asyncio.to_thread` does not stop the thread, so the abandoned reader steals
   the next chunk.
2. **The PTY is closed on every path**, including the idle timeout and a browser
   that vanishes. Closing it is also what releases the blocked read.
   `PtySocket` also clears the 60 s timeout docker-py leaves on the hijacked
   exec socket: left in place, `recv` raises after a minute of silence, the pump
   reads that as the shell exiting, and every idle terminal dies after sixty
   seconds regardless of the idle policy. A read timeout is never EOF.
3. **Nothing is logged or recorded.** The control API keeps who/when/how long
   (ADR 0023); a keystroke in a log line here would undo that.

The endpoint requires the node token in the handshake (`Sec-WebSocket-Protocol`
or the `token` query parameter), read from `NESTBOX_NODE_TOKEN_FILE` when one is
configured and from `NESTBOX_NODE_TOKEN` otherwise, and refuses every connection
when neither is set: the runner is also on the sandbox network, so an
open terminal endpoint there would be a shell into every neighbour.
`NESTBOX_TERMINAL_IDLE_SECONDS` exists so a test can watch the idle close in
seconds instead of half an hour.

## Files

| File | Responsibility |
|---|---|
| `app/main.py` | The FastAPI adapter |
| `app/terminal.py` | The browser terminal: node token, PTY pump, resize, idle close |
| `tests/test_terminal.py` | The pump and the frame rules, without FastAPI and without Docker (`python3 -m unittest discover -s tests -t .`) |
| `Dockerfile` | Image build |

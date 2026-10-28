# runtime/ — execution

## Purpose

The boundary between deciding and doing. A service says "start this sandbox";
this layer knows how that is actually performed, and nothing above it knows.

## Why a trait

The product runs on Docker today and on OpenShell gateways next. Keeping the
adapter behind `RuntimeAdapter` means that change is one file, and it means the
services can be tested against `FakeAdapter` without a container runtime.

## Security rules

1. A command is an argument vector. No shell string is ever built from a name, a
   label, or anything else that came from a request.
2. Only images from the approved catalogue may be started; the sandbox service
   rejects an unpinned image before this layer is reached, and this layer refuses
   one too rather than trusting the caller.
3. A new container runs without privileges: no privileged mode, no host network,
   dropped capabilities, resource limits set.
4. The adapter holds the only path to the container socket, and that socket is
   mounted into the runner, not into the API.

## Files

| File | Responsibility |
|---|---|
| `mod.rs` | The `RuntimeAdapter` trait (create, inspect, list, lifecycle, logs, exec, hardware, processes, inventory, build) and its errors |
| `http_runner.rs` | Adapter that speaks to the sandbox runner's internal API |
| `fake.rs` | In-memory adapter for tests |

## Extending

Implement `RuntimeAdapter`, add it to the construction in `main.rs`, and say in
the module docs which guarantees it does *not* provide. A fake that quietly
satisfies more than the real thing hides bugs.

## Hardware and builds

`hardware()` returns the runner host's raw `/proc/cpuinfo`, `/proc/meminfo`,
and `nvidia-smi` text plus whether the Docker daemon has an NVIDIA runtime.
Raw rather than parsed, because the parsers are tested Rust in
`services/model_service.rs`. `build()` sends a generated Dockerfile and gets an
image tag back; the runner only accepts tags under `nestbox-recipe/`.

## Live reads

`processes()` and `inventory()` are questions about a container *now*, so they
are calls rather than stored state: the runner answers with `docker top` and with
the output of a probe script that ships in its own image.

Both are parameterless on the container side. The `ps` column set and the probe
script are the runner's, never the request's, so neither an agent inside the
container nor a caller outside it can steer what is read. The process table comes
back typed and capped (the runner says whether it cut it); the inventory comes
back as the probe's document, because the runner may gain a section and the table
it lands in has fixed columns — `services/inventory_service.rs` is what rebuilds
it by allow-list, and that rebuild is where a value would be rejected.

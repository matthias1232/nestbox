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
| `mod.rs` | The `RuntimeAdapter` trait (create, inspect, list, lifecycle, logs, exec) and its errors |
| `http_runner.rs` | Adapter that speaks to the sandbox runner's internal API |
| `fake.rs` | In-memory adapter for tests |

## Extending

Implement `RuntimeAdapter`, add it to the construction in `main.rs`, and say in
the module docs which guarantees it does *not* provide. A fake that quietly
satisfies more than the real thing hides bugs.

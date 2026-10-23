Nestbox is the control plane of a one-boot AI appliance. It wraps the NVIDIA OpenShell gateway,
so you manage sandboxes ("nests"), their policies, their reachable endpoints and the appliance
itself through one HTTP API instead of a CLI.

## Authentication

Two schemes are accepted on every endpoint:

- `Token` — `Authorization: Token <key>`. Create one with `POST /api/v1/identity/tokens/`
  (email and password), or in the UI under *Account → API tokens*. Revoke a token to cut off
  exactly one client.
- Session — the browser cookie used by the web UI.

## Authorization

Three roles exist: `admin`, `operator`, `viewer`. Each operation in this document carries
`x-roles` (who may call it) and `x-effect` (`read`, `write` or `destructive`), so a client can
render or hide actions without guessing.

## Errors

Every failure uses one envelope, including streaming endpoints:

```json
{"error": {"code": "sandbox.not_found", "message": "Nest 'x' does not exist.", "details": {}, "request_id": "…"}}
```

`code` is stable and safe to branch on. `message` is English and for developers; the UI
translates the code, never the message.

## Streaming

Endpoints marked `x-stream: sse` emit Server-Sent Events. Event names are part of the contract
(`containers`, `sandboxes`, `build-log`), and each event's `data` is JSON.

## Versioning

`/api/v1/` is frozen: breaking changes get a new prefix. `/api/schema/v1/` is the machine-readable
contract for it, and CI fails when the committed contract changes without intent.

# nestbox/ — the operator CLI

## Purpose

The same API the console uses, from a terminal, for the operations an operator
repeats: list the supervisors, read one sandbox's settings, queue a check or a
policy apply, tail a log.

## Rules

- **HTTP only.** Every command is one call to the published API with a session,
  a CSRF token on writes, and the same error codes. The CLI holds no rule the
  API does not have.
- **Configuration from the environment**, never from arguments that end up in a
  shell history: `NESTBOX_URL`, `NESTBOX_EMAIL`, `NESTBOX_PASSWORD`.
- **Exit codes are the contract**: `0` success, `1` a request failed, `2` a
  usage error. A script can branch on them without parsing prose.
- **Help needs no login.** `nestbox help` answers before a client is built.

## Files

| File | Responsibility |
|---|---|
| `main.rs` | Argument dispatch, exit codes, JSON printing |
| `api.rs` | Login, cookie, CSRF header, request and error mapping |
| `commands.rs` | The commands: supervisors and sandboxes |

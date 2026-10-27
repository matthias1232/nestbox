# ADR 0023 — A terminal session is audited by who and when, never by what

Status: Accepted
Date: 2026-10-27

## Context

The Admin Center offers a browser terminal: xterm.js over a WebSocket to a PTY in
the sandbox. The obvious next question is whether to record it. Recording is easy
— the bytes are already in the process — and it turns the terminal into a place
where an operator's `cat` of a credential file becomes part of the product's
permanent record.

The alternative, recording nothing, makes "who opened a shell in that sandbox"
unanswerable, which is the question an incident actually asks.

## Decision

**Record the session, not its content.** `sandbox_sessions` holds the sandbox, the
user, the kind (`terminal` or `ssh`), the open and close time, the close reason
(`idle`, `user`, `error`, `sandbox_stopped`), and byte counts in each direction.
There is no column for input or output, and there will not be one.

**The control API is the only place the session exists.** The browser talks to the
control API, the control API talks to the runner over the internal network with a
node token. A session that bypassed the API would be a session without an audit
row, so the runner's socket is not reachable from the console network.

**Limits are explicit.** Thirty minutes without input closes the session with
`idle`. Four terminals per sandbox and twenty per user, so a stuck tab cannot
exhaust the runner. A close frame names the reason, so the browser can say why
instead of showing a dead cursor.

**The Origin header is checked** against the configured origins, because a
WebSocket is not covered by the same-origin policy: without the check, any page a
logged-in operator visits could open a shell in their name.

## Alternatives rejected

**Full session recording (asciinema-style).** It is a keystroke log of production
access, and a keystroke log is a credential log the first time someone types a
token.

**No audit table at all.** Then the answer to "who had a shell in the sandbox
that leaked" is "unknown", which is the answer that makes an audit useless.

**PTY inside the control API process.** It would put a Docker exec socket in the
API, which is the boundary `sandbox-runner` exists to hold.

## Consequences

- An incident review can say who, when, and for how long, and cannot say what they
  ran. That is a real limit and it is stated in the console's Access tab.
- Byte counts are a rough proxy for activity, not a transcript.
- The terminal is a WebSocket, which the OpenAPI document marks `x-stream: ws` and
  the console's nginx needs its own location for (the SSE location sends an empty
  `Connection` header on purpose).

## What would invalidate this

A regulator or a customer contract requiring command-level audit for privileged
sandboxes would force a different answer — most likely a small, allowlisted set of
audited actions rather than a raw transcript, and a new ADR superseding this one.

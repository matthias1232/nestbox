# traffic/ — the live console

## Purpose

The promise: see what a sandbox is doing **now**, not after a refresh.

## How it works

- One SSE connection per page, opened once and closed on unmount.
- Events are appended to a bounded buffer. An unbounded buffer is a memory leak
  that only appears after an hour, which is exactly when an operator is watching
  an incident.
- Pause stops the rendering, not the connection, so resuming shows what happened
  while paused.

## Rules

- The stream is filtered by kind on the client and by environment on the server.
  The server filter is the one that matters for safety; the client filter is for
  readability.
- A rule is never created from a click. A click creates a **draft**, which is
  reviewed and approved (ADR 0011).
- A rendering error must not close the stream.

## Files

| File | Responsibility |
|---|---|
| `useTrafficStream.js` | Connection, buffer, pause, filters |
| `TrafficPage.jsx` | Table, controls, draft dialog |

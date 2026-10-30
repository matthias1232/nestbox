# traffic/ — the live console

## Purpose

The promise: see what **every** sandbox is doing **now**, and see the minutes
before it. One table, one window, one stream.

## How it works

- Opening the page reads `GET /api/v1/traffic?window=…` — the environment-wide
  window that already happened — and then opens the SSE stream. The stream only
  carries what arrives after it opened, so without the read an idle sandbox looks
  like a broken page and a reload loses the last minutes.
- Both sources are normalised to one row shape. The stream carries less than the
  stored row: a live event knows the destination and the decision, a stored row
  also knows the method, path and MCP tool. The table shows the difference rather
  than an empty cell that looks like a bug.
- Rows are de-duplicated by event id: a re-read and a repeated frame are the same
  observation.
- The buffer is capped. An unbounded buffer is a memory leak that appears an hour
  into an incident, which is exactly when an operator is watching.
- Pause stops rendering, not the connection, so resuming shows what happened.

## Filters

- **Decision** (`blocked`/`allowed`) and **sandbox** are server filters: changing
  one re-reads the window, so the counts and the rows describe the same thing.
- **Kind** and the **search box** are view filters over the loaded rows.
- `dns` rows are their own event kind on the server. A DNS row is badged with the
  `dns` channel; it never opened a connection.

## Rules

- A rule is never created from a click. A click creates a **draft**, which is
  reviewed and approved (ADR 0011).
- A rendering error must not close the stream.
- Every row names its sandbox. The question is "what is happening, and where";
  an id alone does not answer it.

## Files

| File | Responsibility |
|---|---|
| `useTrafficStream.js` | History read, stream, normalisation, buffer, pause, filters |
| `TrafficPage.jsx` | Controls, facets, table, draft dialog |

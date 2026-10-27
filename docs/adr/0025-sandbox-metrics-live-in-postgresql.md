# ADR 0025 — Sandbox metrics live in PostgreSQL, raw for a day and rolled up for a week

Status: Accepted
Date: 2026-10-27

## Context

The Admin Center draws CPU, memory, network, and disk per sandbox over 1 h, 24 h,
and 7 d. Fifty sandboxes sampled every five seconds is 864 000 rows a day. The
appliance is one machine with one database, and ADR 0001 made PostgreSQL the only
control store.

The obvious move is a metrics system — Prometheus, VictoriaMetrics — because that
is what the tool is for. The obvious risk is a second store, a second query
language, a second retention policy, and a second thing to back up.

## Decision

**Metrics are tables in PostgreSQL.** The runner samples `docker stats` every five
seconds and posts batches; `sandbox_metrics` keeps raw points for 24 h in daily
partitions, and `sandbox_metrics_1m` keeps one-minute averages for 7 d. Retention
drops a partition instead of deleting rows.

**The console asks for a window, and the server picks the table.** `range=1h`
reads raw samples; `24h` and `7d` read the rollup. The answer is bounded (720,
1 440, 10 080 points), so a week never arrives as a million rows.

**The rollup is a job, not a trigger.** A slow rollup must not slow an insert, and
re-running it for a bucket is idempotent: same aggregate, same row.

**Ingest is node-token authenticated and scoped.** A batch is accepted only for
sandboxes that node runs; a sample older than the raw retention is dropped rather
than written into a partition that is about to disappear.

**Live numbers go over SSE, not polling.** `kind=metric` on the existing stream,
so one channel carries traffic, metrics, and policy events and the console does
not open a request per graph per second.

## Alternatives rejected

**Prometheus plus a remote-read endpoint.** A second store with its own retention,
its own backup, and a query language the console would have to speak. At the
appliance's scale it is more moving parts for the same four lines.

**Only rollups, no raw points.** Cheaper, and it makes the 1 h graph lie about
spikiness: a 30-second CPU spike disappears into an average.

**Let the browser sample on demand.** The sample would only exist while a tab is
open, so the graph would have gaps exactly when nobody was watching — which is
when an incident starts.

## Consequences

- 864 000 rows a day is the size being carried, and the partition maintenance job
  is what keeps it from growing without bound.
- A metrics query is a normal SQL query under the tenant scope, so tenant
  isolation is the same policy as everything else.
- `blk_read`/`blk_write` are cumulative counters; the rate is derived when the
  points are drawn, and the console labels them as rates.
- If a node's clock jumps, a sample can land in the wrong partition; the default
  partition catches it instead of failing the insert.

## What would invalidate this

A fleet large enough that the write volume competes with control-plane traffic
would justify a separate store — at which point the metrics read path, not the
schema, is what changes. One appliance is nowhere near that line.

# ADR 0006 — Cursor paging and server-side facets

Status: Accepted
Date: 2026-10-26

## Context

The requirement is to manage thousands of sandboxes. The prototype returned
the full list and let the browser filter it. That is fine at ten rows and
unusable at ten thousand: the payload is large, the client does the work, and
the same filter logic is implemented twice with subtly different semantics.

## Decision

Every list endpoint is server-paged, server-filtered, server-sorted, and
returns facets.

```json
{
  "items": [ ... ],
  "next_cursor": "eyJ0cyI6MTcyOTg2NDUwMCwidCI6ImFiYyJ9",
  "total_estimate": 10432,
  "facets": { "status": { "running": 8123, "stopped": 2309 } }
}
```

- **Keyset (cursor) paging**, not `OFFSET`. The cursor encodes the last sort
  key and the tiebreaker id.
- **Stable sort** with an `id` tiebreaker, so equal timestamps cannot cause a
  row to appear twice or be skipped.
- **Facets** are computed in the same round trip as the page, so the filter UI
  shows counts without a second request per facet.
- **`total_estimate`** comes from the planner estimate, not `COUNT(*)`, because
  an exact count over ten thousand rows on every page is wasted work.
- Filter syntax is identical in the API and the UI, so there is one set of
  semantics.

## Alternatives rejected

**`OFFSET` paging.** Rejected: page N scans and discards N×page rows, so deep
pages degrade linearly, and concurrent inserts shift rows between pages. It is
the single most common reason a list view becomes slow.

**Fetch everything and filter in the browser.** Rejected: it is the current
state, and it is the thing that breaks at scale. It also duplicates filter
logic and makes the API and UI disagree.

**Separate endpoint per facet.** Rejected: N requests per filter change, and
they can disagree with each other under concurrent writes.

**Exact `COUNT(*)` for totals.** Rejected as the default: it turns a fast page
into a full scan. An estimate is honest for a UI that says "about 10,400" and
can be made exact on demand via a parameter.

## Consequences

- A cursor is opaque, so clients cannot construct one. That is intended: it
  keeps the encoding free to change.
- Sorting must always include a unique tiebreaker, or paging can duplicate or
  skip rows. This is enforced in the query builder rather than left to the
  caller.
- An index must exist for each supported sort order, otherwise keyset paging
  degrades to a sort. The migrations name them explicitly.
- `total_estimate` can be wrong after bulk changes until statistics refresh.
  The UI labels it as approximate.

## What would invalidate this

- A requirement for jump-to-page navigation, which keyset paging does not
  support. That would need a hybrid approach, not a return to `OFFSET`
  everywhere.

//! Sandbox resource history: ingest, query, rollup, retention.
//!
//! The rules this module owns, so they are in one place rather than in a query:
//!
//! * a batch from a node is accepted only for sandboxes that node runs, and a
//!   sample older than the raw retention is dropped rather than written;
//! * `range=1h` reads raw samples, `24h` and `7d` read the one-minute rollup —
//!   the console gets the number of points it asked for, not a million;
//! * the rollup job is idempotent: re-running it for a bucket overwrites the
//!   same row with the same aggregate.
//!
//! The `feat/sandbox-metrics` package fills this module.

//! Sandbox resource samples and their rollup.
//!
//! Two tables, two cadences (`0020_sandbox_metrics.sql`):
//!
//! * `sandbox_metrics` — raw `docker stats` points every 5 s, daily
//!   partitions, 24 h retention;
//! * `sandbox_metrics_1m` — one-minute averages, 7 d retention, written only by
//!   the `metrics_rollup` job.
//!
//! The `feat/sandbox-metrics` package fills this module: the batch insert into
//! the current partition, the ranged read that picks raw or rollup by window,
//! the rollup upsert, and the partition maintenance that creates days ahead and
//! drops days behind. Every query is bounded by the window it serves, so a
//! console that asks for `7d` reads at most 10 080 buckets.

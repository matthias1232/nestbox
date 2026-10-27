//! The request and response bodies behind a traffic row.
//!
//! `traffic_bodies` (`0022_egress.sql`) is separate from `telemetry_events` on
//! purpose: bodies are large and live 24 h, the hot event table is small and
//! lives longer. A row here is keyed by the event id, is capped at 64 KiB per
//! side by the proxy, and has **already passed the central redactor** before it
//! is inserted (`security::redaction`). That is the only promise a "log
//! everything" switch can honestly make, and it is made in one place.
//!
//! The `feat/proxy-telemetry` package fills this module: the insert on ingest,
//! the read for one event, and the retention sweep that the
//! `egress_partition_maintenance` job drives.

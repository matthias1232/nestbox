//! The compiled policy set the proxy enforces, and the decisions it recorded.
//!
//! Three tables (`0022_egress.sql`):
//!
//! * `policy_compilations` — the hash of the rule set last compiled for an
//!   environment, so "deployed" is a comparison rather than an opinion:
//!   `nodes.policy_hash_reported == policy_compilations.hash`;
//! * `policy_checks` — one row per decision the proxy asked for, partitioned by
//!   day, 30 d retention;
//! * `traffic_windows` — the open "allow everything" windows, with a ceiling
//!   (`max_duration_minutes`) so one cannot be left open by accident.
//!
//! The `feat/policy-decider` package fills this module. The read path for a
//! decision is one query: the enabled rules for the environment plus the open
//! windows, so the decider never asks twice for the same request.

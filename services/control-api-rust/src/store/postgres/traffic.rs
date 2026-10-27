//! The traffic log as the console reads it: rows, aggregates, and map edges.
//!
//! Reads across `telemetry_events` (`kind = 'network' | 'dns' | 'policy'`),
//! `policy_checks`, and `mcp_tool_calls` for one sandbox and one window. Every
//! query carries both `environment_id` and `sandbox_id`, and every one is
//! bounded by `ts` with `LIMIT`: an unbounded scan over a week of traffic is
//! exactly the query that would take the console down with it.
//!
//! The `feat/live-traffic` package fills this module, which owns:
//!
//! * the paginated row list with filters (blocked only, MCP only, window);
//! * the map edges aggregated per destination for the last 5 min;
//! * the per-destination counts that let the console say "1 204 allowed,
//!   7 blocked" without counting rows itself.

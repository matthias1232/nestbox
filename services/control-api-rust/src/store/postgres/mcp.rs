//! MCP servers and the calls the proxy saw.
//!
//! Three tables (`0023_mcp.sql`):
//!
//! * `mcp_registry` — the server definitions every sandbox can be given, with
//!   the transport that decides whether the proxy ever sees a call;
//! * `sandbox_mcp_servers` — which sandbox has which server, its status, and
//!   the tool list discovery returned;
//! * `mcp_tool_calls` — one row per JSON-RPC call on the wire, partitioned by
//!   day, 7 d retention.
//!
//! The `feat/mcp-registry` package fills this module. Credentials are not here
//! and never will be: a remote server's token lives in the agent's own home
//! directory inside the container.

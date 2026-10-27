//! MCP servers: the registry, a sandbox's attachment to them, and discovery.
//!
//! Transport decides visibility, and this module has to say so honestly rather
//! than imply coverage it does not have: an `http` or `sse` server crosses the
//! egress proxy, so its calls appear in traffic with a tool name; a `stdio`
//! server is a child process inside the container and is not egress. The
//! console shows the difference, so "no calls" means "no calls", not "we cannot
//! see this kind of server".
//!
//! Discovery (`tools/list`) is an outbound request like any other: it goes
//! through the same rule check, and a sandbox without an allow rule cannot
//! discover anything. That is intentional — a discovery path that bypasses the
//! policy would be an egress hole with a friendly name.
//!
//! The `feat/mcp-registry` package fills this module.

//! How an operator reaches a sandbox: SSH keys, published ports, inventory, and
//! the session audit.
//!
//! Four tables (`0021_sandbox_access.sql`, `0024_agent_ports.sql`):
//!
//! * `sandbox_ssh_keys` — the public half, a `file://` locator for the private
//!   half, and the fingerprint. The private key is a 0600 file on the node and
//!   never a column (ADR 0005);
//! * `node_port_pool` — a claimed port per sandbox and purpose, unique per node
//!   so two sandboxes cannot be published on the same port;
//! * `sandbox_inventory` — the last snapshot from the fixed runner script;
//! * `sandbox_sessions` — who opened a terminal or an SSH session, when, and for
//!   how long. Never the content.
//!
//! The `feat/sandbox-access` package fills this module. Port claiming is a
//! single statement with `ON CONFLICT DO NOTHING` and a retry, not a
//! select-then-insert: the unique constraint is what makes it safe.

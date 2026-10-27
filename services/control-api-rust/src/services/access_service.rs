//! Reaching a sandbox from outside: SSH keys, published ports, sessions.
//!
//! The SSH key pair is generated here, written as a 0600 file under the node's
//! secret directory, and recorded as a locator (ADR 0005). The private half
//! leaves the server exactly once, through the one type built for it
//! (`domain::access::SshKeyDownload`); a second download answers 410 until the
//! key is rotated, and rotation deletes the old file before writing the new one.
//!
//! A published port is claimed from the node's pool with the unique constraint
//! doing the arbitration, and released when the forward is withdrawn. The
//! runner is told to publish `-p <node-address>:<port>:<target>`, so what the
//! Access tab shows is what `docker ps` shows.
//!
//! The `feat/sandbox-access` package fills this module.

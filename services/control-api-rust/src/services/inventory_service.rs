//! What is installed inside a sandbox, as a snapshot.
//!
//! The inventory comes from one fixed script in the runner image: no
//! parameters, no user input, and no way for the agent inside the container to
//! change what it prints. This module stores the result and answers with the
//! snapshot plus its age, because "Hermes 0.22.5" is only useful with "as of
//! four minutes ago".
//!
//! A value of an environment variable never enters the snapshot: the script
//! prints names, and the type (`domain::access::Inventory`) has no field that
//! could hold a value. A test asserts that a `sk-…` string in a fake probe
//! answer does not reach the stored JSON.
//!
//! The `feat/sandbox-metrics` package fills this module.

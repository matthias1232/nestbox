//! The live event bus.
//!
//! The SSE response itself is built in `api::events`, because producing an HTTP
//! response is the API layer's job. This module owns the bus and the visibility
//! rule, which is the part that must not depend on a transport.

pub mod bus;

pub use bus::{is_visible_to, EventBus, LocalBus, StreamEnd};

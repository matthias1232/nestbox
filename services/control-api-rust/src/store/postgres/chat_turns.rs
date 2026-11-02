//! Rows of `chat_turns`: insert, query, group.
//!
//! The statements live here so the service has no SQL and the store has no rules.

use crate::error::StoreError;
use crate::store::TenantScope;

//! Rows of `privacy_exceptions`: insert, read, delete.
//!
//! Matching is a service concern; this file only moves rows.

use crate::error::StoreError;
use crate::store::TenantScope;

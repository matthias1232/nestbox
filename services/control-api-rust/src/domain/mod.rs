//! Domain vocabulary: the types every other layer is written against.
//!
//! Nothing here performs I/O. That is not a style preference: it is what makes
//! a business rule testable in microseconds and what stops an execution model
//! from leaking into the model of the problem.

pub mod auth;
pub mod environment;
pub mod gateway;
pub mod job;
pub mod model;
pub mod pagination;
pub mod policy;
pub mod provider;
pub mod runtime;
pub mod sandbox;
pub mod telemetry;

pub use pagination::{Cursor, ListParams, Page, SortDirection};

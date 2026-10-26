//! Geschäftslogik – ohne HTTP, ohne Axum.
//!
//! Jedes Modul hier bekommt den [`AppState`] und fachliche Typen. Dadurch sind
//! die Regeln unabhängig von der Transport-Schicht testbar.

pub mod sandboxes;
pub mod providers;
pub mod reference;
pub mod tools;
pub mod topology;

//! The surface API: one implementation of every ability, below every screen
//! (§AR-009-surfaces).
//!
//! §REQ-002-parity says the command line and the interface offer the same
//! abilities. This module is how that is true by construction: it has readings,
//! which return a view and change nothing, and moves, which take a request and
//! return an outcome. A surface chooses what to show and which key reaches
//! which call; it never computes an answer of its own (§AR-001-layers.1).

pub mod act;
pub mod conversation;
pub mod offers;
pub mod parity;
pub mod read;
pub mod schema;
pub mod session;
pub mod views;

pub use session::{JobSubject, OrgInfo, Session, WorkLines};

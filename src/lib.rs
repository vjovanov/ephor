//! ephor as a library.
//!
//! The binary is the ordinary way to use ephor. This library exists for the
//! in-process half of §FS-001-forge-interface.2: implementing [`forge::Forge`]
//! in Rust. Rust has no stable plugin ABI, so an in-process implementation
//! living outside this repository means depending on this crate, implementing
//! the trait, and building a binary that registers it — there is nothing to
//! dynamically load. An implementation that would rather not do that writes an
//! executable instead and is adapted by [`forge::external::ExternalForge`];
//! both answer the same types and the same policy runs over both.

pub mod agents;
pub mod api;
pub mod attribution;
pub mod branches;
pub mod burn;
pub mod capabilities;
pub mod checkout;
pub mod cli;
pub mod commands;
pub mod doctor;
pub mod error;
pub mod feed;
pub mod forest;
pub mod forge;
pub mod git;
pub mod given;
pub mod grounds;
pub mod hooks;
pub mod manifest;
pub mod matter;
pub mod paths;
pub mod rebase;
pub mod registry;
pub mod scope;
pub mod seams;
pub mod table;
pub mod ticket_ids;
pub mod update;
pub mod work;

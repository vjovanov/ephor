//! Per-project information stream: cached feed of PRs, CI, messages, and
//! custom status, fetched by pluggable providers.

pub mod cache;
pub mod commands;
pub mod config;
pub mod gate;
pub mod model;
pub mod provider;
pub mod providers;
pub mod reachability;
pub mod react;
pub mod refresh;
pub mod render;
pub mod tui;

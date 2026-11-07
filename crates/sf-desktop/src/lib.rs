//! Desktop-shell security policy for SafiData (specification sections 8.6 and 10.2).
//!
//! This crate is pure Rust with zero network access (AGENTS.md rule 7). It holds
//! the rules the Tauri 2.2 shell in `apps/desktop/src-tauri` must enforce:
//! local-only content, a strict `safi://` deep-link allowlist, an IPC command
//! allowlist with a payload ceiling, and an explicit Tauri capability allowlist.
//!
//! Every rejection carries a stable `SF-SYS-*` code from `errors.toml`; the
//! messages themselves are never duplicated here (rules 8 and 9).

pub mod capabilities;
pub mod commands;
pub mod content;
pub mod deep_link;
pub mod error;

pub use error::{DesktopError, DesktopErrorCode};

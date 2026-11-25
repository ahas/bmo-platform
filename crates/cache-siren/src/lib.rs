//! Agency's keeping of the accounts' usage windows and their readers (docs/usage-windows.md), and its
//! cache-lifetime judgement (docs/sessions.md, 「A worker's session per model」).
//!
//! Derived from the cache-siren project, carried forward through bmo-cache-siren; the repository's
//! NOTICE states the derivation and the crate's licence standing.
//!
//! The keeping and the judgement know no provider: each reader is one provider's member and declares its
//! own provider, cadence and freshness, and a provider member hands in the lifetime it declares.

pub mod balance;
pub mod claude;
pub mod codex;
pub mod lifetime;
pub mod reader;
pub mod windows;

pub use balance::{Balance, KeptBalance};
pub use reader::Reader;
pub use windows::{FULL, Keeping, KeptWindow, Reading, Slot, Window};

/// The version the siren names itself at to the codex app-server: the set's version the build was made at,
/// which the testbed hands the build as `BMO_VERSION`, else the manifest's placeholder (bmo-chat's
/// `docs/conventions.md`, 「Versions」).
pub const VERSION: &str = match option_env!("BMO_VERSION") {
  Some(version) => version,
  None => env!("CARGO_PKG_VERSION"),
};

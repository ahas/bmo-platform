//! The agent host (docs/agent-control.md, 「The host」): one process per data root running every
//! structured execution, each under a supervisor of its own.

pub mod ask;
pub mod disk;
pub mod envelope;
pub mod host;
pub mod identity;
pub mod layout;
pub mod place;
pub mod policy;
pub mod signals;
pub mod supervise;
pub mod trust;
pub mod view;

//! Agency's store (docs/store.md): the PostgreSQL database agency keeps its tables in, where it is, the pools
//! and the read-only connection a process opens on it, the one way a statement is bound and run, the registry's
//! readers, the settings more than one reader takes, a supervisor's reset point, the migrations that make its
//! tables and the seeds a fresh store is given.

pub mod connection;
pub mod migration;
pub mod place;
pub mod registry;
pub mod reset_point;
pub mod seed;
pub mod settings;
pub mod statements;

pub use connection::{connect, connect_lazy, number, open_read, pool, retrying, server_went_away};
pub use statements::{Fault, all, one, plan_outlived, replanned, run, shape_moved, statement};

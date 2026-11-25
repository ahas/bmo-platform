//! The queue's mechanics agency keeps (docs/worker-slots.md, docs/agent-control.md, 「Stopping」, 「Asking
//! whether it is alive」): the workers and what they may be, a worker's pin and a job's roads, what runs and
//! what a stop and a cancellation reach, the open executions closed on evidence, what a job's end leaves in
//! herdr, the settings rows these keep their marks in, and the paths a worker takes. What a job is, when it
//! runs and what it is told are the job unit's.

pub mod cancels;
pub mod holds;
pub mod leftovers;
pub mod orderer;
pub mod paths;
pub mod pins;
pub mod reconcile;
pub mod routes;
pub mod running;
pub mod settings;
pub mod stops;
pub mod workers;

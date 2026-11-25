//! The common co-worker pool's dispatch and the standing branch checkouts (docs/coworker-pool.md): a task's
//! place asked of the pool, admitted, bound and given back; the places a branch works in; and the pool's own
//! clocks. What a consumer's work is, its orders and its jobs, arrives with each request.

pub mod clocks;
pub mod dispatch;
pub mod places;

//! What the modules of the test harness share: a workspace binary built for a test to start, a gate standing in
//! for the published one, a scratch store of a test's own and a ground seeded on one, git run under a neutral
//! author, the supervisor and repository rows a test lays down, agency's own server standing on one on a seat of
//! the test server's connections, a front that cuts or holds the calls to a stand-in, a stand-in for a place's
//! program and one for the session backend.

pub mod binary;
pub mod connections;
pub mod cut;
pub mod gates;
pub mod git;
pub mod grounds;
pub mod peers;
pub mod program;
pub mod rows;
pub mod scratch;
pub mod stand_in;
pub mod standing;
pub mod wording;

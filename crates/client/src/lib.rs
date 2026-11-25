//! The agency service's client (`docs/openapi.json`): every body and member of its API as a Rust type, and
//! `AgencyClient`, one method for each route. The service's OpenAPI document is generated from these types.

mod client;
mod tools;
mod types;

pub use client::{AgencyClient, Answer, ClientFailure, ListAccountsQuery, OrgUnitRemovalQuery};
pub use tools::{FILE_ITEM, REVIEW_JOB, STOP_JOB, TOOLS, Tool};
pub use types::*;

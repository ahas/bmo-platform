//! The repository's one test harness (bmo-chat's docs/conventions.md, 「Rust builds」): each crate's tests a
//! module of it, the checks that span the repository a module each, and what they share in `support`. Every
//! module shares the process; a test that sets what stands for the whole of it runs alone in a process of its
//! own (`integration::common::alone`).

mod support;

mod agent_host;
mod cache_siren;
mod deploy_quiet;
mod first_lessons;
mod harness_machine;
mod harness_providers;
mod herdr;
mod integration;
mod layout;
mod literals;
mod llm_gateway;
mod node_link;
mod pane_head;
mod policy;
mod server;
mod store;
mod tools;
mod typescript_binaries;

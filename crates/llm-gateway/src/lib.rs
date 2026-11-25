//! The model gateway (docs/own-llm.md, 「The gateway」): the launcher the unit `bmo-llm-gateway` runs.
//! It reads the registry out of the store, writes LiteLLM's table from it under the data root, starts
//! LiteLLM on it as a child on a loopback port of its own, and is itself the front every run and every
//! gate asks. A table that changed is a new child, which takes over once it answers while the old one
//! finishes the requests it holds.

pub mod child;
pub mod costs;
pub mod door;
pub mod front;
pub mod keys;
pub mod streams;
pub mod supply;
pub mod table;

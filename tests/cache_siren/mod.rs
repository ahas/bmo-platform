//! The cache-siren crate's tests: the keeping of the accounts' usage windows on a scratch store of the
//! test's own, the readers that fill it against stand-in endpoints and programs, and the cache-lifetime
//! judgement.

mod balance;
mod claude;
mod codex;
mod lifetime;
mod windows;

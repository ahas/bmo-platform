//! Agency's side of the tool host's call protocol (docs/tools.md, 「A tool's folder」), which every tool agency
//! provides shares: the one request a placed executable is handed on standard input, the parts of its session
//! record agency's tools read, the turn the request is made in, how agency's server is reached, what a call is
//! answered with, and the tree the tools read agency's `prompts/` from.

pub mod account_service;
mod answer;
mod control;
mod handed;
mod session;
mod tree;

pub use answer::Answer;
pub use control::Control;
pub use handed::{Asked, Handed, PROTOCOL, answer_standard_io, read_handed};
pub use session::{
  AGENT_VARIABLE, BEGAN_VARIABLE, CHAT_VARIABLE, DATA_ROOT_VARIABLE, Labels, Session, Turn, VIA_VARIABLE,
};
pub use tree::prompts_tree;

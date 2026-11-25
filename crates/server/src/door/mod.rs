//! The resident door (docs/agent-control.md, 「What a resident turn reads」, 「The order for putting in
//! one turn and waiting for it to end」): one turn put into an agent's standing program and read back
//! out of the record that program writes, which the words said to a worker, the keepalive and the task
//! runner put their turns through. A twin check holds it to the TypeScript pane turn
//! (the agent package's `worker-pane.ts`).

pub mod apart;
pub mod aside;
pub mod asked;
pub mod confirm;
pub mod endpoint;
pub mod execution;
pub mod guard;
pub mod inbox;
pub mod liveness;
pub mod looks;
pub mod node_take;
pub mod place_run;
pub mod record;
pub mod record_secrets;
pub mod rollout;
pub mod stall;
pub mod stand;
pub mod surface;
pub mod tool_clock;
pub mod turn;
pub mod written_call;

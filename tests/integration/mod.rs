//! Agency's integration tests, one module to an area: the verbs of `bmo-agency`
//! through their functions and their clap subcommands, the places against a herdr of the test's own, the
//! standing workers, the stops, the logins and the co-worker pool's hook, each over a store of its own. The
//! modules share the process; a test that sets what stands for the whole of it runs alone in a process of
//! its own (`common::alone`).

pub(crate) mod common;

mod agent_status_verb;
mod agents_verb;
mod claude_logins;
mod dev_model_verbs;
mod door_guard;
mod machine_memory;
mod machine_verbs;
mod nouns;
mod places;
mod record_secrets_verbs;
mod runs;
mod slot_release;
mod stops;
mod subcommands;
mod supervisor_follow_verb;
mod supervisor_reset_verb;
mod work_node_hub;
mod work_node_verbs;

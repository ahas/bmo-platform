//! A person's PC (docs/person-nodes.md): the ports the far end of the link stands on there, the verbs its home
//! bridge runs here, and the grants and capabilities a supervisor of no account holds, which reach every node and
//! operate the platform.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersonNodes {
  pub mirror_port: u16,
  pub herdr_port: u16,
  pub wait_seconds: u64,
  /// How long a run waits on its node's link before it ends failed, in seconds.
  pub wait_bound_seconds: u64,
  /// The resources every tool session issued for a supervisor of no account holds.
  pub administrator_grants: Vec<String>,
  /// The capabilities that operate the platform, which every tool session issued for a supervisor of no account
  /// holds and no other caller's does.
  pub administrator_capabilities: Vec<String>,
  /// The verbs of the CLI a person's node may ask the home bridge to run here.
  pub bridge_verbs: Vec<String>,
  /// How often the PCs heard again are looked for, to keep their shared apps' copies, in seconds.
  pub copies_seconds: u64,
  /// How long after a setup, sync or check that left a person's PC not ready it is stood up again, in seconds.
  pub setup_again_seconds: i64,
}

crate::part!(pub PERSON_NODES: PersonNodes = "person-nodes.toml");

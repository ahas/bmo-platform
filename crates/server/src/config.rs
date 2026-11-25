//! The deployment's configuration: the names its agent and its owner go by (docs/policy.md, 「The
//! deployment's names」), where each service agency calls is reached (docs/api.md, 「Calls agency
//! makes」), the arbiter's master among them, the brain repository the agents' brain folders stand in (docs/agent-ownership.md, 「Long-term
//! memory」), the file
//! the persona's placeholder values are kept in (docs/api.md, 「Calls agency makes」), and the trees its
//! release is read from (docs/agent-control.md, 「The job's release」). Each is read through the variables
//! the deployment names it in.

use std::path::{Path, PathBuf};

use harness_machine::card_master::MasterAddress;
use harness_providers::Machine;
use policy::deployment::{DEPLOYMENT_NAMES, Deployment};

use crate::peers::{PeerAddress, brain, gate, node};

/// The configuration a process runs under.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
  pub names: Deployment,
  pub peers: PeerAddresses,
  /// The brain repository where the deployment names one; `brain_root` reads it with its default.
  pub brain: Option<PathBuf>,
  /// The file the persona's placeholder values are kept in, none where the deployment names none.
  pub persona_values: Option<PathBuf>,
  pub trees: Trees,
}

/// The variable naming the brain repository.
pub const BRAIN_ROOT: &str = "AGENCY_BRAIN_ROOT";

/// The variable naming the jake hub's executable, whose connect mode carries a link to a person's PC
/// (docs/person-nodes.md, 「The link over jake」).
pub const JAKE_HUB: &str = "AGENCY_JAKE_HUB";

/// The variable naming the file the persona's placeholder values are kept in.
pub const PERSONA_VALUES: &str = "AGENCY_PERSONA_VALUES";

// The brain repository under the data root where the deployment names none.
const BRAIN_UNDER_DATA: &str = "brain";

/// The variables naming the trees the server's release is read from (docs/agent-control.md, 「The job's
/// release」).
pub const CODE_DIR: &str = "AGENCY_CODE_DIR";
pub const DEPLOY_DIR: &str = "AGENCY_DEPLOY_DIR";
pub const SOURCE_DIR: &str = "AGENCY_SOURCE_DIR";

// Agency's own installation directory under the home, where its deploy tree's link stands unless named.
// It holds nothing of another service's; the data root agency shares with them stands elsewhere.
const INSTALLED_UNDER_HOME: &str = ".local/share/bmo-agency";

/// The trees the deployment names, each none where its variable is unset or blank: the code tree it
/// started the server on, the deploy tree's link and the base checkout.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Trees {
  pub code: Option<PathBuf>,
  pub deploy: Option<PathBuf>,
  pub source: Option<PathBuf>,
}

impl Trees {
  fn read(variable: &impl Fn(&str) -> Option<String>) -> Self {
    let named = |name: &str| {
      variable(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
    };

    Self {
      code: named(CODE_DIR),
      deploy: named(DEPLOY_DIR),
      source: named(SOURCE_DIR),
    }
  }

  /// The trees as a machine's environment names them.
  pub fn of_machine(machine: &Machine) -> Self {
    Self::read(&|name| machine.var(name).map(str::to_owned))
  }

  /// The deploy tree's link: the named one, else `deploy` under agency's own installation directory in
  /// the home.
  pub fn deploy_link(&self, home: &Path) -> PathBuf {
    self.deploy.clone().unwrap_or_else(|| home.join(INSTALLED_UNDER_HOME).join("deploy"))
  }

  /// The base checkout: the named one, else the working directory.
  pub fn source_root(&self) -> PathBuf {
    self.source.clone().unwrap_or_else(|| std::env::current_dir().unwrap_or_default())
  }
}

/// Where each service agency calls is reached, or none where the deployment names no address for it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PeerAddresses {
  pub node: Option<PeerAddress>,
  pub brain: Option<PeerAddress>,
  pub gate: Option<gate::GateAddress>,
  /// The arbiter's master, which grants the cards a registration naming a card service runs on
  /// (docs/own-llm.md, 「An engine the arbiter stands」).
  pub arbiter: Option<MasterAddress>,
  /// The account key a brain agency makes is filed under, none where the deployment names none.
  pub brain_account: Option<String>,
}

impl Config {
  /// The configuration read through `variable`; an address that does not read as a URL is a fault
  /// naming its variable.
  pub fn read(variable: impl Fn(&str) -> Option<String>) -> Result<Self, String> {
    Ok(Self {
      names: DEPLOYMENT_NAMES.read(&variable),
      peers: PeerAddresses {
        node: PeerAddress::read(&node::ADDRESS, &variable)?,
        brain: PeerAddress::read(&brain::ADDRESS, &variable)?,
        gate: gate::GateAddress::read(&variable)?,
        arbiter: MasterAddress::read(&variable)?,
        brain_account: variable(brain::ACCOUNT)
          .map(|value| value.trim().to_owned())
          .filter(|value| !value.is_empty()),
      },
      brain: variable(BRAIN_ROOT)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from),
      persona_values: variable(PERSONA_VALUES)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .map(PathBuf::from),
      trees: Trees::read(&variable),
    })
  }

  /// The brain repository: the folder the deployment names, else `brain` under the data root.
  pub fn brain_root(&self, data_root: &Path) -> PathBuf {
    self.brain.clone().unwrap_or_else(|| data_root.join(BRAIN_UNDER_DATA))
  }

  /// The configuration this process's environment names.
  pub fn of_process() -> Result<Self, String> {
    Self::read(|name| std::env::var(name).ok())
  }
}

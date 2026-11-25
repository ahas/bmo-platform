//! The brain (docs/api.md, 「Calls agency makes」) through the client it publishes: the brain an agent is
//! given as it is registered, and what its reset and removal take of what it remembers. Everything an agent
//! remembers is its own brain, found by the agent's key, so agency lets that brain go whole; a preview only
//! finds it. A chat launch reads the persona that brain renders of the layers it names.

use std::sync::Arc;

use bmo_brain_client::{
  Brain as BrainMember, BrainClient, BrainKind, BrainsQuery, ClientFailure, ConsolidationQuery,
  MemoriesQuery, NewBrain, NewMemory, PersonaQuery,
};
use serde::Deserialize;
use serde_json::Value;

use super::{Asking, PeerAddress, PeerFault, Variables};

/// Where the deployment names the brain.
pub const ADDRESS: Variables = Variables {
  url: "AGENCY_BRAIN_URL",
  token: "AGENCY_BRAIN_TOKEN",
};

/// The variable naming the account key a brain agency makes is filed under.
pub const ACCOUNT: &str = "AGENCY_BRAIN_ACCOUNT";

const PEER: &str = "brain";

/// What a reset of an agent took of the brain, or would take: the brains let go, counted, and what it could
/// not do.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct BrainReset {
  #[serde(default)]
  pub took: Value,
  #[serde(default)]
  pub could_not: Vec<String>,
}

/// What became of the ask to make an agent's brain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Made {
  /// The brain made one.
  Made,
  /// The brain already kept one under the agent's key, so nothing was made.
  Kept,
  /// No brain is deployed, so none was asked for.
  Undeployed,
}

/// What the agent's brain rendered of its persona.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rendered {
  /// The rendering, empty where no layer named stands in the brain.
  Text(String),
  /// The agent has no brain.
  NoBrain,
  /// No brain is deployed, so none was asked.
  Undeployed,
}

/// The label a sleep's summary carries, its value the day the sleep was for (docs/sleep.md, 「The run」).
pub const SLEEP_LABEL: &str = "sleep";

/// The type a sleep's summary is written as.
pub const SLEEP_TYPE: &str = "reflection";

/// Who a sleep's summary is written as.
pub const SLEEP_ACTOR: &str = "sleep";

/// What an agent's brain holds to sleep on: its unconsolidated episodes, and the rows written since its last
/// sleep (all of them where it never slept).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Pending {
  pub unconsolidated: i64,
  pub written_since: i64,
}

impl Pending {
  /// Whether there is anything to sleep on.
  pub fn any(&self) -> bool {
    self.unconsolidated > 0 || self.written_since > 0
  }
}

/// What agency asks the brain.
pub trait Brain: Send + Sync {
  /// The agent's own brain made under its key, named as given, where the brain keeps none yet: filed under
  /// the id of the login account owning it where one is named, and the deployment's account otherwise.
  fn make<'a>(&'a self, agent: &'a str, name: &'a str, owner: Option<i64>) -> Asking<'a, Made>;
  /// The agent's brain let go whole, once nothing writes there.
  fn forget<'a>(&'a self, agent: &'a str) -> Asking<'a, ()>;
  /// The agent's brain taken whole or, for a preview, found and counted with nothing taken.
  fn reset<'a>(&'a self, agent: &'a str, preview: bool) -> Asking<'a, BrainReset>;
  /// The discipline a supervisor's system prompt carries: how it reads and writes its brain every turn.
  fn discipline(&self) -> Asking<'_, String>;
  /// The persona the agent's brain renders of the layers named, comma-separated.
  fn persona<'a>(&'a self, agent: &'a str, layers: &'a str) -> Asking<'a, Rendered>;
  /// What the agent's brain holds to sleep on; none where the agent has no brain.
  fn pending<'a>(&'a self, _agent: &'a str) -> Asking<'a, Option<Pending>> {
    Box::pin(async { Ok(None) })
  }
  /// A sleep's summary written into the agent's brain: one reflection labelled with the day it slept for.
  fn slept<'a>(&'a self, _agent: &'a str, _date: &'a str, _summary: &'a str) -> Asking<'a, ()> {
    Box::pin(async { Ok(()) })
  }
}

/// No brain deployed: no agent has a brain, so nothing is counted or taken.
pub struct NoBrain;

impl Brain for NoBrain {
  fn make<'a>(&'a self, _agent: &'a str, _name: &'a str, _owner: Option<i64>) -> Asking<'a, Made> {
    Box::pin(async { Ok(Made::Undeployed) })
  }

  fn forget<'a>(&'a self, _agent: &'a str) -> Asking<'a, ()> {
    Box::pin(async { Ok(()) })
  }

  fn reset<'a>(&'a self, _agent: &'a str, _preview: bool) -> Asking<'a, BrainReset> {
    Box::pin(async { Ok(BrainReset::default()) })
  }

  fn discipline(&self) -> Asking<'_, String> {
    Box::pin(async { Ok(String::new()) })
  }

  fn persona<'a>(&'a self, _agent: &'a str, _layers: &'a str) -> Asking<'a, Rendered> {
    Box::pin(async { Ok(Rendered::Undeployed) })
  }
}

/// The brain asked through its published client, and the account key a brain agency makes is filed under
/// where the deployment names one.
pub struct PublishedBrain {
  client: BrainClient,
  account: Option<String>,
}

impl PublishedBrain {
  pub fn new(address: PeerAddress, account: Option<String>) -> Result<Self, String> {
    let client = BrainClient::new(address.url.as_str(), address.token.unwrap_or_default())?;

    Ok(Self { client, account })
  }

  /// The agent's brain made where none is kept under its key, filed under its owner's account or the
  /// deployment's: refused where neither names one.
  async fn made(&self, agent: &str, name: &str, owner: Option<i64>) -> Result<Made, PeerFault> {
    if self.find(agent).await?.is_some() {
      return Ok(Made::Kept);
    }

    let Some(account) = owner.map(|owner| owner.to_string()).or_else(|| self.account.clone()) else {
      return Err(PeerFault::Refused {
        peer: PEER,
        code: "no-account".into(),
        message: format!("{ACCOUNT} names no account key to file the brain of {agent} under"),
      });
    };
    let asked = NewBrain {
      account_key: account,
      agent_key: agent.to_owned(),
      kind: Some(BrainKind::Agent),
      name: Some(name.to_owned()).filter(|name| !name.is_empty()),
    };

    self.client.brain_add(&asked).await.map_err(fault)?;

    Ok(Made::Made)
  }

  /// The one agent brain registered for the agent's key, where one is; an org brain never answers for it.
  async fn find(&self, agent: &str) -> Result<Option<BrainMember>, PeerFault> {
    let query = BrainsQuery {
      kind: Some(BrainKind::Agent),
      agent_key: Some(agent.to_owned()),
      limit: Some(1),
      ..BrainsQuery::default()
    };
    let page = self.client.brains(&query).await.map_err(fault)?;

    Ok(page.items.into_iter().next())
  }

  /// The agent's brain found and, where asked, let go: how many brains went, none where it has none.
  async fn let_go(&self, agent: &str, preview: bool) -> Result<BrainReset, PeerFault> {
    let Some(brain) = self.find(agent).await? else {
      return Ok(BrainReset::default());
    };

    if !preview {
      match self.client.brain_let_go(brain.id).await.map_err(fault) {
        Ok(_) | Err(PeerFault::Failed { status: 404, .. }) => {}
        Err(why) => return Err(why),
      }
    }

    Ok(BrainReset {
      took: serde_json::json!({ "brains": 1 }),
      could_not: vec![],
    })
  }

  /// The persona the agent's brain renders of the layers named, where the agent has a brain; a layer that
  /// does not stand in it is read as an empty rendering.
  async fn rendered(&self, agent: &str, layers: &str) -> Result<Rendered, PeerFault> {
    let Some(brain) = self.find(agent).await? else {
      return Ok(Rendered::NoBrain);
    };
    let query = PersonaQuery {
      layers: Some(layers.to_owned()),
    };

    match self.client.persona(brain.id, &query).await.map_err(fault) {
      Ok(text) => Ok(Rendered::Text(text)),
      Err(PeerFault::Failed { status: 404, .. }) => Ok(Rendered::Text(String::new())),
      Err(why) => Err(why),
    }
  }
}

impl PublishedBrain {
  /// The unconsolidated episodes and the rows written since the newest sleep summary the agent's brain holds.
  async fn pending_in(&self, agent: &str) -> Result<Option<Pending>, PeerFault> {
    let Some(brain) = self.find(agent).await? else {
      return Ok(None);
    };
    let last = self
      .client
      .memories(
        brain.id,
        &MemoriesQuery {
          kind: Some(SLEEP_TYPE.to_owned()),
          status: Some("active".to_owned()),
          label: Some(SLEEP_LABEL.to_owned()),
          limit: Some(1),
          record: Some(false),
          ..MemoriesQuery::default()
        },
      )
      .await
      .map_err(fault)?;
    let since = last.items.first().map_or(0, |summary| summary.created_at);
    let state = self
      .client
      .consolidation_since(brain.id, &ConsolidationQuery { since: Some(since) })
      .await
      .map_err(fault)?;

    Ok(Some(Pending {
      unconsolidated: state.unconsolidated_count,
      written_since: state.written_since.unwrap_or(0),
    }))
  }

  /// The sleep's summary written as a first version, where the agent has a brain.
  async fn summary_in(&self, agent: &str, date: &str, summary: &str) -> Result<(), PeerFault> {
    let Some(brain) = self.find(agent).await? else {
      return Ok(());
    };
    let written = NewMemory {
      kind: SLEEP_TYPE.to_owned(),
      text: summary.to_owned(),
      importance: Some(3),
      actor_name: SLEEP_ACTOR.to_owned(),
      labels: [(SLEEP_LABEL.to_owned(), date.to_owned())].into_iter().collect(),
      ..NewMemory::default()
    };

    self.client.memory_add(brain.id, &written).await.map_err(fault)?;

    Ok(())
  }
}

fn fault(why: ClientFailure) -> PeerFault {
  match why {
    ClientFailure::Answered {
      status,
      answer: Some(error),
    } => PeerFault::Failed {
      peer: PEER,
      status,
      code: error.code,
      error: error.error,
    },
    ClientFailure::Answered {
      status,
      answer: None,
    } => PeerFault::Failed {
      peer: PEER,
      status,
      code: String::new(),
      error: String::new(),
    },
    ClientFailure::Unreachable(detail) => PeerFault::Unreachable { peer: PEER, detail },
    ClientFailure::Unreadable(detail) => PeerFault::Malformed { peer: PEER, detail },
  }
}

impl Brain for PublishedBrain {
  fn make<'a>(&'a self, agent: &'a str, name: &'a str, owner: Option<i64>) -> Asking<'a, Made> {
    Box::pin(async move { self.made(agent, name, owner).await })
  }

  fn forget<'a>(&'a self, agent: &'a str) -> Asking<'a, ()> {
    Box::pin(async move { self.let_go(agent, false).await.map(|_| ()) })
  }

  fn reset<'a>(&'a self, agent: &'a str, preview: bool) -> Asking<'a, BrainReset> {
    Box::pin(async move { self.let_go(agent, preview).await })
  }

  fn discipline(&self) -> Asking<'_, String> {
    Box::pin(async move { self.client.discipline().await.map_err(fault) })
  }

  fn persona<'a>(&'a self, agent: &'a str, layers: &'a str) -> Asking<'a, Rendered> {
    Box::pin(async move { self.rendered(agent, layers).await })
  }

  fn pending<'a>(&'a self, agent: &'a str) -> Asking<'a, Option<Pending>> {
    Box::pin(async move { self.pending_in(agent).await })
  }

  fn slept<'a>(&'a self, agent: &'a str, date: &'a str, summary: &'a str) -> Asking<'a, ()> {
    Box::pin(async move { self.summary_in(agent, date, summary).await })
  }
}

/// The client the address chooses, filing a brain it makes under the account named.
pub fn client(address: Option<PeerAddress>, account: Option<String>) -> Arc<dyn Brain> {
  let Some(address) = address else {
    return Arc::new(NoBrain);
  };

  match PublishedBrain::new(address, account) {
    Ok(brain) => Arc::new(brain),
    Err(why) => {
      eprintln!("agency: the brain's address did not read: {why}");

      Arc::new(NoBrain)
    }
  }
}

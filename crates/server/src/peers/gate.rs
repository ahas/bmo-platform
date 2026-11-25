//! The gate (docs/api.md, 「Calls agency makes」) through the client it publishes: the reference a job's
//! first prompt is handed, picked over its instruction by the gate's reference road. The reference is the knowledge, rules and documents of the brain of the supervisor the call names,
//! chosen by the gate; a worker and a voyager read the brain only through what the gate hands them here. And
//! the English of what a person wrote for a job's worker, read by the gate's translator (docs/worker-language.md,
//! 「What agency translates on the way in」).

use std::sync::Arc;

use bmo_gate_client::{Entry, GateClient, Reading, ReferenceAsk, TranslationAsk};
use url::Url;

use super::{Asking, PeerFault, Variables};

/// Where the deployment names the gate.
pub const ADDRESS: Variables = Variables {
  url: "BMO_GATE_URL",
  token: "BMO_GATE_TOKEN_FILE",
};

const PEER: &str = "gate";

/// Where the gate is reached, and the token it is called with read out of the file the deployment names,
/// none where it names none.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GateAddress {
  pub url: Url,
  pub token: Option<String>,
}

impl GateAddress {
  /// The address `variables` name through `variable`: none where the address is unset or blank, and a
  /// fault naming the variable where it does not read as an HTTP URL. The token is the file's content,
  /// read as it is named, an unreadable or unnamed file being no token.
  pub fn read(variable: impl Fn(&str) -> Option<String>) -> Result<Option<Self>, String> {
    let named =
      |name: &str| variable(name).map(|value| value.trim().to_owned()).filter(|value| !value.is_empty());
    let Some(url) = named(ADDRESS.url) else {
      return Ok(None);
    };
    let url = Url::parse(&url).map_err(|why| format!("{}: {url} is not an address: {why}", ADDRESS.url))?;

    if !matches!(url.scheme(), "http" | "https") {
      return Err(format!("{}: {url} is not an HTTP address", ADDRESS.url));
    }

    let token = named(ADDRESS.token)
      .and_then(|path| std::fs::read_to_string(path).ok())
      .map(|token| token.trim().to_owned())
      .filter(|token| !token.is_empty());

    Ok(Some(Self { url, token }))
  }
}

/// What agency asks the gate.
pub trait Gate: Send + Sync {
  /// The reference the gate picks for a job's instruction: the knowledge, rules
  /// and documents of `supervisor`'s brain its lines call for, each with its path, text and source. Empty
  /// where the gate is undeployed or does not answer.
  fn reference<'a>(&'a self, supervisor: &'a str, lines: Vec<String>) -> Asking<'a, Vec<Entry>>;

  /// The gate kinds and the order they stand in, as the gate publishes them: an empty order where the gate
  /// declares that no gate stands, and none where no gate is deployed to publish one.
  fn kinds<'a>(&'a self) -> Asking<'a, Option<Vec<bmo_gate_client::Gate>>> {
    Box::pin(async { Ok(None) })
  }

  /// Each original read into `language`, in the order given, each whole or with the parts the translator
  /// could not render standing as written. A gate that translates nothing answers that it cannot.
  fn translate<'a>(&'a self, language: &'a str, originals: Vec<String>) -> Asking<'a, Vec<Reading>> {
    let _ = (language, originals);

    Box::pin(async {
      Err(PeerFault::Unreachable {
        peer: PEER,
        detail: "no gate translates here".to_owned(),
      })
    })
  }
}

/// A prompt with the reference the gate picked attached, its entries under a heading of their own, each
/// named by its path and source; the prompt stands alone where no entry was picked.
pub fn attach(prompt: &str, entries: &[Entry]) -> String {
  if entries.is_empty() {
    return prompt.to_owned();
  }

  let mut text = prompt.to_owned();

  text.push_str("\n\n## Reference\n\n");

  for entry in entries {
    text.push_str(&format!("`{}` ({})\n\n{}\n\n", entry.path, entry.source, entry.text));
  }

  text
}

/// No gate deployed: nothing is asked, so a job and a thrown-away session carry no picked reference.
pub struct NoGate;

impl Gate for NoGate {
  fn reference<'a>(&'a self, _supervisor: &'a str, _lines: Vec<String>) -> Asking<'a, Vec<Entry>> {
    Box::pin(async { Ok(Vec::new()) })
  }
}

/// The gate asked through its published client.
pub struct PublishedGate {
  client: GateClient,
}

impl PublishedGate {
  pub fn new(address: &GateAddress) -> Result<Self, String> {
    let client = GateClient::new(address.url.as_str(), address.token.clone().unwrap_or_default())
      .map_err(|why| why.to_string())?;

    Ok(Self { client })
  }

  async fn reference_selected(
    &self,
    supervisor: &str,
    lines: Vec<String>,
  ) -> Result<Vec<Entry>, PeerFault> {
    // A job carries no room, no speakers and no situation: the envelope names the supervisor the
    // turn belongs to, whose brain the road reads, and a turn with no line asks none of it.
    let ask = ReferenceAsk {
      room_id: 0,
      message_id: None,
      lines,
      speaker_ids: vec![],
      situation: None,
      agent: Some(supervisor.to_owned()),
      holds: None,
      round: None,
      people_spoke: false,
      answered: None,
      records: false,
      // No room, so no audience: the gate reads it as the internal one a turn with no situation is.
      audience: None,
    };

    self.client.reference(&ask).await.map(|answer| answer.entries).map_err(fault)
  }
}

fn fault(why: bmo_gate_client::ClientFailure) -> PeerFault {
  match why {
    bmo_gate_client::ClientFailure::Answered {
      status,
      answer: Some(error),
      ..
    } => PeerFault::Failed {
      peer: PEER,
      status,
      code: error.code,
      error: error.error,
    },
    bmo_gate_client::ClientFailure::Answered {
      status,
      answer: None,
      ..
    } => PeerFault::Failed {
      peer: PEER,
      status,
      code: String::new(),
      error: String::new(),
    },
    bmo_gate_client::ClientFailure::Unreachable(detail) => PeerFault::Unreachable { peer: PEER, detail },
    bmo_gate_client::ClientFailure::Unreadable(detail) => PeerFault::Malformed { peer: PEER, detail },
  }
}

impl Gate for PublishedGate {
  fn reference<'a>(&'a self, supervisor: &'a str, lines: Vec<String>) -> Asking<'a, Vec<Entry>> {
    Box::pin(async move { self.reference_selected(supervisor, lines).await })
  }

  fn kinds<'a>(&'a self) -> Asking<'a, Option<Vec<bmo_gate_client::Gate>>> {
    Box::pin(async move { self.client.gates().await.map(|page| Some(page.items)).map_err(fault) })
  }

  fn translate<'a>(&'a self, language: &'a str, originals: Vec<String>) -> Asking<'a, Vec<Reading>> {
    Box::pin(async move {
      let ask = TranslationAsk {
        language: language.to_owned(),
        originals,
      };

      self.client.translate(&ask).await.map(|answer| answer.readings).map_err(fault)
    })
  }
}

/// The client the address chooses, filing nothing where none is named or the address does not read.
pub fn client(address: Option<GateAddress>) -> Arc<dyn Gate> {
  let Some(address) = address else {
    return Arc::new(NoGate);
  };

  match PublishedGate::new(&address) {
    Ok(gate) => Arc::new(gate),
    Err(why) => {
      eprintln!("agency: the gate's address did not read: {why}");

      Arc::new(NoGate)
    }
  }
}

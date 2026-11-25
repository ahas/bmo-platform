//! Gates a test hands agency in place of the published one: none, which translates nothing; one publishing the
//! order it is given; and a translator that reads each original out of a table the test sets, writes down every
//! call it is asked, and can be told to answer a reading only in part or never to answer.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use agency_server::peers::gate::{Gate, NoGate};
use agency_server::peers::{Asking, PeerFault};
use bmo_gate_client::Reading;

/// No gate deployed.
pub fn none() -> Arc<dyn Gate> {
  Arc::new(NoGate)
}

/// How the stand-in translator answers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Answering {
  /// Each original's reading out of the table, whole; an original the table lacks answered as it stands, whole.
  #[default]
  Whole,
  /// Each original's reading out of the table, not whole.
  InPart,
  /// No answer ever comes.
  Never,
}

/// A translator reading originals out of a table, with the calls it was asked.
#[derive(Default)]
pub struct Translator {
  pub readings: BTreeMap<String, String>,
  pub answering: Answering,
  /// Each call's language and originals, in the order asked.
  pub asked: Mutex<Vec<(String, Vec<String>)>>,
}

impl Translator {
  /// A translator answering whole from the pairs given, each an original and its reading.
  pub fn reading(pairs: &[(&str, &str)]) -> Self {
    Self {
      readings: pairs
        .iter()
        .map(|(original, reading)| ((*original).to_owned(), (*reading).to_owned()))
        .collect(),
      ..Self::default()
    }
  }

  /// The same translator answering as told.
  pub fn answering(self, answering: Answering) -> Self {
    Self { answering, ..self }
  }

  /// The calls asked so far.
  pub fn calls(&self) -> Vec<(String, Vec<String>)> {
    self.asked.lock().unwrap().clone()
  }
}

impl Gate for Translator {
  fn reference<'a>(
    &'a self,
    _supervisor: &'a str,
    _lines: Vec<String>,
  ) -> Asking<'a, Vec<bmo_gate_client::Entry>> {
    Box::pin(async { Ok(Vec::new()) })
  }

  fn translate<'a>(&'a self, language: &'a str, originals: Vec<String>) -> Asking<'a, Vec<Reading>> {
    self.asked.lock().unwrap().push((language.to_owned(), originals.clone()));

    let readings: Vec<Reading> = originals
      .iter()
      .map(|original| Reading {
        text: self.readings.get(original).cloned().unwrap_or_else(|| original.clone()),
        whole: self.answering == Answering::Whole,
      })
      .collect();
    let answering = self.answering;

    Box::pin(async move {
      if answering == Answering::Never {
        std::future::pending::<()>().await;
      }

      Ok::<_, PeerFault>(readings)
    })
  }
}

/// A gate that publishes the order it is given and picks no reference.
pub struct Declaring(pub Vec<bmo_gate_client::Gate>);

impl Gate for Declaring {
  fn reference<'a>(
    &'a self,
    _supervisor: &'a str,
    _lines: Vec<String>,
  ) -> Asking<'a, Vec<bmo_gate_client::Entry>> {
    Box::pin(async { Ok(Vec::new()) })
  }

  fn kinds<'a>(&'a self) -> Asking<'a, Option<Vec<bmo_gate_client::Gate>>> {
    let order = self.0.clone();

    Box::pin(async move { Ok(Some(order)) })
  }
}

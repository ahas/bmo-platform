//! The shape of the gates a turn passes, as the gate declares them for a chat turn (docs/api.md, 「gate
//! kinds」). Agency keeps no order of its own; the gate's declaration is its.

use serde::Deserialize;

/// A gate's place in a step: the gate, the condition its step runs under where it runs in some turns
/// only, and whether it is one of the gates that pick a turn's reference.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Place {
  pub key: String,
  #[serde(default)]
  pub condition: Option<String>,
  #[serde(default)]
  pub reference: bool,
}

/// One step of a turn part: the gates the turn starts together and waits on together.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Step {
  pub places: Vec<Place>,
}

/// The gates one kind of turn passes, in the two parts a turn has.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnGates {
  pub pre: Vec<Step>,
  pub post: Vec<Step>,
}

impl TurnGates {
  /// Every gate the turn passes, once, in the first part it stands in and in the turn's order, with
  /// that part's name.
  pub fn gates(&self) -> Vec<(&'static str, &str)> {
    let mut seen = std::collections::HashSet::new();

    [("pre", &self.pre), ("post", &self.post)]
      .into_iter()
      .flat_map(|(part, steps)| {
        steps.iter().flat_map(|step| &step.places).map(move |place| (part, place.key.as_str()))
      })
      .filter(|(_, key)| seen.insert(*key))
      .collect()
  }
}

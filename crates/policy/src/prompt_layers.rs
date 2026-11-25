//! The layers of a supervisor's system prompt between its harness and its role, and the persona's choice
//! (docs/prompt-layers.md): the platform's rules' folder, the person's own instructions, and the settings agency
//! declares on the account service's delegation chain for them.

use serde::Deserialize;

/// One setting agency declares on the delegation chain, in the fields the account service takes a kind in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Kind {
  pub key: String,
  pub description: String,
  pub shape: String,
  #[serde(default)]
  pub choices: Vec<String>,
  pub mode: String,
  pub rules: bool,
  pub levels: Vec<String>,
  pub unit_permission: String,
  pub person_writer: String,
  /// The value standing where no link says one, or none.
  #[serde(default)]
  pub default: Option<toml::Value>,
}

/// The organisation's instructions: the lines that open and close them in a prompt, and their kind.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Organisation {
  pub header: String,
  pub end: String,
  pub kind: Kind,
}

/// The persona's choice: the two keys it chooses between, and its kind.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Persona {
  pub own: String,
  pub none: String,
  pub kind: Kind,
}

/// The person's own instructions: the lines that open and close them in a prompt, whether a person may give
/// them, and the kind they are kept under on the person's own link.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Personal {
  pub header: String,
  pub end: String,
  pub allowed: Kind,
  pub text: Kind,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PromptLayers {
  /// The unit name agency declares its kinds under.
  pub declared_by: String,
  /// The folder under `prompts/` the platform's rulebooks stand in.
  pub platform_folder: String,
  /// What a supervisor the platform's rules do not reach is told its own folder is.
  pub workplace_unnamed: String,
  /// How long one ask of the account service may take, in seconds.
  pub ask_seconds: u64,
  pub organisation: Organisation,
  pub persona: Persona,
  pub personal: Personal,
}

impl PromptLayers {
  /// Every kind agency declares.
  pub fn kinds(&self) -> [&Kind; 4] {
    [&self.organisation.kind, &self.persona.kind, &self.personal.allowed, &self.personal.text]
  }
}

crate::part!(pub PROMPT_LAYERS: PromptLayers = "prompt-layers.toml");

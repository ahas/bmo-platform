//! The pieces role and the supervisors' limits (docs/supervisors.md, 「What a turn is given」, 「Its
//! settings」), and the generation settings their turns are asked with (「Its generation settings」).

use serde::Deserialize;

use crate::model_catalogue::NumberBounds;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupervisorRules {
  /// The role every turn of the pieces shape is given, in its fixed order.
  pub pieces_role: Vec<String>,
  /// The platform's rulebooks a supervisor of the platform's administrator reads on a turn of the pieces shape.
  pub pieces_platform: Vec<String>,
  /// The longest a persona the settings hold may be, in characters.
  pub persona_most: usize,
  /// The pattern a supervisor is registered by.
  pub name_pattern: String,
  /// The most call names a supervisor's row may hold.
  pub call_names_most: usize,
  /// The longest a call name may be, in characters once trimmed.
  pub call_name_longest: usize,
  /// The mark a screen puts beside a supervisor added on the console.
  pub added_icon: String,
  /// Whether a supervisor's turns are given their reference where its settings never held the switch.
  pub reference_on: bool,
}

crate::part!(pub SUPERVISORS: SupervisorRules = "supervisors.toml");

/// One generation setting, by the name the engine reads it under. Its bounds are read flattened, which
/// serde reads only without refusing unknown fields.
#[derive(Debug, Deserialize)]
pub struct GenerationSetting {
  pub name: String,
  /// Whether it takes on or off rather than a number.
  #[serde(default)]
  pub switch: bool,
  /// Whether the chat template reads it, so a request carries it under `chat_template_kwargs`.
  #[serde(default)]
  pub template: bool,
  /// Its bounds where it takes a number; a switch has none.
  #[serde(flatten)]
  pub bounds: Option<NumberBounds>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationSettings {
  pub settings: Vec<GenerationSetting>,
}

impl GenerationSettings {
  pub fn setting(&self, name: &str) -> Option<&GenerationSetting> {
    self.settings.iter().find(|setting| setting.name == name)
  }
}

crate::part!(pub GENERATION: GenerationSettings = "generation.toml");

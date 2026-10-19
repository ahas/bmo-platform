//! What a line is said with once it is made a script for a voice (`lang/<language>/voice-script.yaml`): the
//! words markup is unfolded into, the particles fitted to the sound before them, the words a figure is read
//! in, and why a gate's answer was not used, each line with the `{name}` marks its file names.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceScript {
  pub spoken: SpokenWords,
  pub particles: ParticleWords,
  pub figures: FigureWords,
  pub gate: GateWords,
  pub reading: ReadingWords,
  pub polish: PolishWords,
}

/// The words a line's markup is unfolded into.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpokenWords {
  pub ordinals: Vec<String>,
  pub title_particle: String,
  pub address: String,
  pub address_words: Vec<String>,
  pub polite_endings: Vec<String>,
  pub attached: String,
  pub attached_polite: String,
  pub attached_english: String,
}

/// The particles that change with the sound before them, each as its closed form and its open form.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParticleWords {
  pub pairs: Vec<[String; 2]>,
  pub open_after_rieul: Vec<[String; 2]>,
}

/// The words a figure is read in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FigureWords {
  pub digits: Vec<String>,
  pub places: Vec<String>,
  pub groups: Vec<String>,
  pub native_ones: Vec<String>,
  pub native_tens: Vec<String>,
  pub twenty: String,
  pub first: String,
  pub months: BTreeMap<String, String>,
  pub native_counters: Vec<String>,
  pub counter_ends: String,
  pub hour: String,
  pub ordinal: String,
  pub machines: String,
  pub month: String,
  pub months_counted: String,
  pub item: String,
  pub date: String,
  pub hours: String,
  pub minutes: String,
  pub seconds: String,
  pub ratio: String,
  pub fraction: String,
  pub point: String,
  pub dollars: String,
  pub won: String,
  pub plus: String,
  pub minus: String,
  pub range: String,
  pub symbols: BTreeMap<String, String>,
}

/// Why a gate gave nothing to use, whichever gate it was.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GateWords {
  pub no_answer: String,
  pub stopped: String,
  pub could_not_run: String,
  pub unexplained: String,
}

/// Why the reading gate's answer was not a reading of its spans.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReadingWords {
  pub no_reading: String,
  pub roman: String,
  pub figure: String,
}

/// Why the polishing gate's answer may not stand for the script.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolishWords {
  pub no_korean: String,
  pub private_marks: String,
  pub unknown_placeholder: String,
  pub placeholder_count: String,
  pub changed: String,
  pub added: String,
  pub ordinal_missing: String,
  pub too_short: String,
  pub too_long: String,
}

crate::part!(pub VOICE_SCRIPT: VoiceScript = "voice-script");

//! What the voice briefing says where a person or an agent reads it (`lang/<language>/voice.yaml`), each line
//! with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VoiceWords {
  pub switched_off: String,
  pub voiceless: String,
  pub title: String,
  pub unsigned: String,
  pub wordless: String,
  pub not_up: String,
  pub one_switch: String,
  pub usage_hand: String,
  pub usage_brief: String,
  pub runner: RunnerNotes,
}

/// The notes a briefing's run writes in its log as it goes.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunnerNotes {
  pub started: String,
  pub polished: String,
  pub unpolished: String,
  pub unread: String,
  pub read: String,
  pub read_as_written: String,
  pub to_node: String,
  pub node_spoke: String,
  pub node_waited: String,
  pub card_queued: String,
  pub queued: String,
  pub behind_summaries: String,
  pub no_reason: String,
}

crate::part!(pub VOICE_WORDS: VoiceWords = "voice");

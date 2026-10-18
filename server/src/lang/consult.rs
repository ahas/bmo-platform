//! What the consult tool and `bmo consult` answer where an agent or a person reads it
//! (`lang/<language>/consult.yaml`), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsultWords {
  pub empty: String,
  pub no_room: String,
  pub not_answerer: String,
  pub said: String,
  pub number: String,
  pub windows: String,
  pub room_unset: String,
  pub room_shown: String,
  pub room_chosen: String,
  pub not_answered: String,
  pub not_shared: String,
  pub nothing_shown: String,
  pub unanswered: String,
  pub untaken: String,
  pub usage: String,
}

crate::part!(pub CONSULT_WORDS: ConsultWords = "consult");

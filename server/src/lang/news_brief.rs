//! The morning news pages as `bmo news-brief` shows them (docs/news-brief.md, 「The verb」), each line with
//! the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewsBrief {
  pub on: String,
  pub off: String,
  pub line: String,
  pub never: String,
  pub titled_room: String,
  pub removed: String,
  pub no_rooms: String,
  pub holiday_count: String,
  pub holiday_added: String,
  pub holiday_removed: String,
}

crate::part!(pub NEWS_BRIEF: NewsBrief = "news-brief");

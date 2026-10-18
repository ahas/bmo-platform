//! What a song asked for by its title says where a person reads it (`lang/<language>/songs.yaml`), each line
//! with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Songs {
  pub run_title: String,
}

crate::part!(pub SONGS: Songs = "songs");

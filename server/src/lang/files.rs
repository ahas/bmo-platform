//! What a verb of the `bmo` binary that puts a file into a room says where a person reads it
//! (`lang/<language>/files.yaml`), each line with the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Files {
  pub mp3_title: String,
}

crate::part!(pub FILES: Files = "files");

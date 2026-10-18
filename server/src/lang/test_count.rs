//! What `bmo test count` prints (docs/deploy.md, 「The test count」), each line with the `{name}` marks its
//! file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestCountWords {
  pub tree: String,
  pub range: String,
  pub removed: String,
  pub open: String,
}

crate::part!(pub TEST_COUNT: TestCountWords = "test-count");

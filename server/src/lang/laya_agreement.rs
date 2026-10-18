//! What the table of Laya's agreement with each gate's LLM says (docs/laya.md, 「Agreement」), each line with
//! the `{name}` marks its file names.

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayaAgreement {
  pub usage: String,
  pub none: String,
  pub gate: String,
  pub questions: String,
  pub pairs: String,
  pub settled: String,
  pub head: String,
  pub safe_head: String,
}

crate::part!(pub LAYA_AGREEMENT: LayaAgreement = "laya-agreement");

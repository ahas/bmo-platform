//! The deployment's rule file, handed to sumida before anything reads the rule set.

use sumida::api::authz::registry::RuleFile;

/// The deployment's own rules, beside sumida's.
pub const RULES: &[RuleFile] = &[RuleFile {
  name: "bmo.sz",
  text: include_str!("bmo.sz"),
}];

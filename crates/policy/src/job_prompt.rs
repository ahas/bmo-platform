//! What a job's prompt is composed with: the names under a worker's workbench agency keeps
//! (docs/worker-bench.md, 「An item's scratch」), how much of its item's notes it carries, the section naming
//! its lent checkouts, its preamble bundle (docs/worker-bench.md, 「Preamble」), and how what a person wrote for
//! the worker is put into its language (docs/worker-language.md, 「What agency translates on the way in」).

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobPrompt {
  pub workbench: Workbench,
  pub notes: Notes,
  pub lent: Lent,
  pub bench: Bench,
  pub translation: Translation,
}

/// How what a person wrote for a job's worker is put into the worker's language on its way in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Translation {
  /// The language the gate is asked for, by its tag.
  pub language: String,
  /// The most originals one call to the gate carries.
  pub most_originals: usize,
  /// The longest one hand-over waits for the gate's answer.
  pub wait_ms: u64,
  /// The line set over a text that goes in untranslated.
  pub untranslated: String,
  /// The words set after a title that goes in untranslated.
  pub untranslated_title: String,
}

/// The preamble bundle a job's prompt carries under its title: the header's heading and lines, each said where
/// it holds, and the heading the repository's preamble stands under.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bench {
  /// The header's heading.
  pub heading: String,
  /// The job's checkout, with `{checkout}`, `{repository}` and `{path}`.
  pub checkout: String,
  /// A support checkout's number and what the preamble's words for the base mean in it, with `{slot}` and
  /// `{repository}`.
  pub support: String,
  /// A git checkout, with `{path}`, `{head}` and `{origin}`.
  pub git: String,
  /// Where a git checkout stands on a branch, with `{branch}`: its `{head}`.
  pub on_branch: String,
  /// Where a git checkout stands on no branch, with `{commit}`: its `{head}`.
  pub detached: String,
  /// A git checkout's origin, with `{origin}`.
  pub origin: String,
  /// A git checkout with no origin.
  pub no_origin: String,
  /// A directory that is not a git checkout, with `{path}`.
  pub not_checkout: String,
  /// A checkout standing on the item's branch, with `{branch}`.
  pub branch: String,
  /// The workbench folder and the item's scratch in it, with `{workbench}`, `{scratch}` and `{database}`.
  pub workbench: String,
  /// The dev server and its lines, with `{start}`, `{restart}`, `{stop}` and `{log}`.
  pub dev: String,
  /// The heading the repository's preamble stands under.
  pub preamble_heading: String,
}

/// What opens a job's prompt where checkouts of other repositories are lent to it.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lent {
  /// The heading over the list.
  pub heading: String,
  /// The line before the list.
  pub lead: String,
  /// One line a lent checkout, with `{repository}`, `{path}` and `{branch}`.
  pub line: String,
}

/// How much of its item's notes a job's prompt carries.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Notes {
  /// The most bytes of notes a prompt carries, the newest first.
  pub most_bytes: usize,
  /// The line saying how many older notes were left out, with `{n}`.
  pub left_out: String,
}

/// The names under a worker's workbench.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Workbench {
  /// The folder that holds each item's scratch, one folder per item number.
  pub scratch: String,
  /// What an item's own database is named from: this, the item number, an underscore, and anything.
  pub item_database: String,
}

crate::part!(pub JOB_PROMPT: JobPrompt = "job-prompt.toml");

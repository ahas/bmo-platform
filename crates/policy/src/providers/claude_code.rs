//! What Claude Code declares (docs/provider-presence.md).

use std::collections::BTreeMap;

use serde::Deserialize;

use super::Declared;

#[derive(Debug, Deserialize)]
pub struct ClaudeCode {
  #[serde(flatten)]
  pub declared: Declared,
  pub catalogue: Catalogue,
  pub launch: Launch,
  pub cache: Cache,
  pub login: Login,
  pub node_login: NodeLogin,
  pub console_login: ConsoleLogin,
  pub workplace_copy: WorkplaceCopy,
  pub usage: Usage,
  pub advisor: Advisor,
}

/// How every account's own windows are read on a clock (docs/usage-windows.md, 「The Claude account
/// reader」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Usage {
  pub url: String,
  pub pass_seconds: u64,
  pub fresh_passes: u32,
  /// How long an account the endpoint refused for asking too often is left unasked, in seconds, where the
  /// refusal names no moment of its own.
  pub refused_pause_seconds: u64,
  /// The model and effort of the run of the CLI that renews a directory account's login found expired or
  /// refused, and the words that are its turn.
  pub renewal_model: String,
  pub renewal_effort: String,
  pub renewal_words: String,
  /// How long, in seconds, an account that still cannot be read waits after one renewal was asked before
  /// another is.
  pub renewal_pause_seconds: u64,
  /// How close to the moment its credential file states a login is near its end, in seconds: the CLI's
  /// own margin, inside which a run of it renews the token as it goes out.
  pub renewal_margin_seconds: u64,
  pub windows: Vec<String>,
}

/// A directory account signed in from the console through the CLI's own sign-in.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleLogin {
  /// The CLI's sign-in command and its arguments.
  pub command: Vec<String>,
  /// How long the CLI is given to print the sign-in's address, in seconds.
  pub link_seconds: u64,
  /// How long the CLI is given to end once the code is handed over, in seconds.
  pub finish_seconds: u64,
  /// How long a sign-in may stand before it is ended, in minutes.
  pub keep_minutes: u64,
  /// The words the CLI says once a login took, any one of which a sign-in on a person's node, read off its
  /// pane, is judged by (docs/supply.md, 「Signing a person's login in」).
  pub took: Vec<String>,
  /// The words the CLI says once it refused the code, any one of which ends such a sign-in refused.
  pub refused: Vec<String>,
  /// The line a sign-in's pane on a node says where the workplace's own Claude Code does not stand.
  pub missing: String,
}

/// The workplace's own Claude Code on a node (docs/work-nodes.md, 「The workplace on a node」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkplaceCopy {
  /// Where it stands under the workplace's root.
  pub path: String,
  /// The release downloads it is fetched from.
  pub downloads: String,
  /// The variables every program of the workplace runs it with.
  pub env: Vec<(String, String)>,
}

/// Each account's token for the work nodes: where this machine keeps it, the verb that makes one, and
/// how one that stopped answering, or is about to, is told to the owner.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NodeLogin {
  /// The directory under the data root's env directory, one file in it per account, named by its id.
  pub token_dir: String,
  pub command: String,
  /// The leads before a token's year ends, in days, at each of which its ending is told once.
  pub notice_ahead_days: Vec<i64>,
  pub head: String,
  pub gone: String,
  pub missing: String,
  pub ending_in: String,
  pub after: String,
  pub gone_after: String,
}

/// How a login's end is told to the owner: how long before it, the year a token stands for, how long a
/// login that refused a run rests, and the message, one entry to a line, with its blanks named in braces.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Login {
  pub notice_ahead_hours: i64,
  pub token_life_days: i64,
  /// How long before a directory account's login here ends its runs here go out on its token for the nodes.
  pub hand_over_ahead_hours: i64,
  pub refused_rest_minutes: i64,
  /// The entry a login home holds, by which a folder of saved values is told as one.
  pub home_mark: String,
  pub head: String,
  pub gone: String,
  pub ending: String,
  pub ending_in: String,
  pub minutes: String,
  pub hours: String,
  pub run: String,
  pub token_command: String,
  pub directory_command: String,
  pub token_after: String,
  pub directory_after: String,
  pub gone_after: String,
  pub ending_after: String,
  pub carried_on: String,
  pub carries_on: String,
}

/// How long a prefix it holds stays cached, and how long an agent's session may stand idle before it
/// is warmed again.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cache {
  pub ttl_minutes: i64,
  pub keepalive_idle_minutes: i64,
}

/// What every line of it is started with beside its words.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Launch {
  /// The variables every line sets for its part.
  pub variables: BTreeMap<String, String>,
  /// The variables the program reads a login off.
  pub login_variables: Vec<String>,
  /// The variable the program reads its MCP tool idle window from, in milliseconds.
  pub mcp_idle_timeout: String,
  /// What the program writes itself into a session's conversation.
  pub own_words: Vec<String>,
  /// The program's tools whose calls wait long.
  pub long_tools: Vec<String>,
  /// The plugins the program builds in, each named off in every settings file a line hands it.
  pub builtin_plugins: Vec<String>,
  /// The program's own subagent types, each denied on a line whose carries do not keep it.
  pub agent_types: Vec<String>,
}

/// Where under a login's configuration directory the CLI keeps its model catalogue.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Catalogue {
  pub dir: String,
  /// The ending of every file of the surface agency runs on.
  pub suffix: String,
}

/// The advisor a road may name, and the pairings the program attaches (docs/provider-order.md, 「The
/// advisor」).
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Advisor {
  /// The family words a road may name an advisor by, which the catalogue's aliases read as a model.
  pub aliases: Vec<String>,
  pub pairs: Vec<Pair>,
}

/// One pairing: the models, by the start of their name, and the advisors that may advise them, by theirs.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Pair {
  pub main: Vec<String>,
  pub advisors: Vec<String>,
}

impl Advisor {
  /// Whether the word is one of the family words rather than a model.
  pub fn is_alias(&self, word: &str) -> bool {
    self.aliases.iter().any(|alias| alias.eq_ignore_ascii_case(word))
  }

  /// The pairing that takes a model: the one whose start naming it is the longest, none where no start
  /// names it. Names are compared without case.
  pub fn pair_of(&self, model: &str) -> Option<&Pair> {
    let model = model.to_ascii_lowercase();

    self
      .pairs
      .iter()
      .filter_map(|pair| {
        let longest = pair
          .main
          .iter()
          .filter(|start| model.starts_with(&start.to_ascii_lowercase()))
          .map(String::len)
          .max()?;

        Some((longest, pair))
      })
      .max_by_key(|(longest, _)| *longest)
      .map(|(_, pair)| pair)
  }

  /// Whether the program, started on the model with the advisor named, attaches it rather than refusing to start
  /// or dropping it, judged without the catalogue: the model has a pairing, and the advisor is a family word —
  /// read against the catalogue as the road was written — or a model the pairing names.
  pub fn attaches(&self, model: &str, advisor: &str) -> bool {
    self.pair_of(model).is_some() && (self.is_alias(advisor) || self.pairs(model, advisor))
  }

  /// Whether the advisor, a model's name, may advise the model.
  pub fn pairs(&self, model: &str, advisor: &str) -> bool {
    let advisor = advisor.to_ascii_lowercase();

    self.pair_of(model).is_some_and(|pair| {
      pair.advisors.iter().any(|start| advisor.starts_with(&start.to_ascii_lowercase()))
    })
  }
}

crate::part!(pub CLAUDE_CODE: ClaudeCode = "providers/claude-code.toml");

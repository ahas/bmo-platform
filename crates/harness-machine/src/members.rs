//! The gateway's members as the deployment declares them (docs/own-llm.md, 「The gateway's members」,
//! docs/harness-machine.md, 「The members」): the model names the gateway serves beside the registry, the
//! provider each belongs to, its transport, and what a run on it is sized to. Nothing here reads the
//! store: the file is the deployment's, and each side hands its own callers what the file holds.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use policy::registry::REGISTRY;
use regex::Regex;
use serde::Deserialize;
use url::Url;

/// The LiteLLM providers a member may name.
pub const PROVIDERS: [&str; 4] = ["anthropic", "deepseek", "openrouter", "openai"];

/// One member the gateway serves beside the registry. The file names its key by a key file and never by a
/// key.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq)]
pub struct Member {
  /// The model name a caller asks for. Unique among the members and against every registration's names.
  pub name: String,
  /// The agent provider the member belongs to; none where the gateway serves it alone.
  #[serde(default)]
  pub road: Option<String>,
  /// What a screen shows in place of the name.
  #[serde(default)]
  pub label: Option<String>,
  /// The LiteLLM provider, one of `PROVIDERS`.
  pub provider: String,
  /// The id the provider's API knows.
  pub model: String,
  /// The provider's API base, where it has one.
  #[serde(default)]
  pub api_base: Option<String>,
  /// The key file under the data root's `env/`.
  pub key_file: String,
  /// The window a run on the member is sized to, in tokens.
  #[serde(default)]
  pub context_tokens: Option<i64>,
  /// The answer ceiling of a run on the member, in tokens.
  #[serde(default)]
  pub answer_tokens: Option<i64>,
  /// Whether a run on the member thinks by default.
  #[serde(default)]
  pub thinking: Option<bool>,
  /// Whether a run on the member costs nothing; none where the member leaves it to its road's provider.
  #[serde(default)]
  pub free: Option<bool>,
  /// The account the member belongs to, which alone reaches it beside the deployment's own runs; none for
  /// the platform's (docs/supply.md, 「Owned supply」).
  #[serde(default)]
  pub account: Option<i64>,
}

/// A member of a road, with the three values a run on it is sized to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RoadMember<'a> {
  pub member: &'a Member,
  pub context_tokens: i64,
  pub answer_tokens: i64,
  pub thinking: bool,
}

/// Where the deployment's members stand: the path `BMO_LLM_GATEWAY_MEMBERS` names, or
/// `<data root>/llm-gateway/members.json`.
pub fn path(data_root: &Path) -> PathBuf {
  match std::env::var("BMO_LLM_GATEWAY_MEMBERS")
    .ok()
    .map(|named| named.trim().to_owned())
    .filter(|named| !named.is_empty())
  {
    Some(named) => PathBuf::from(named),
    None => data_root.join("llm-gateway").join("members.json"),
  }
}

/// The members the file holds. A file that is not there is no members; a file that does not read, or a
/// member that fails its rules, is a refusal naming the file and why.
pub fn read(path: &Path) -> Result<Vec<Member>, String> {
  let text = match std::fs::read_to_string(path) {
    Ok(text) => text,
    Err(why) if why.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
    Err(why) => return Err(format!("{} cannot be read: {why}", path.display())),
  };
  let members: Vec<Member> =
    serde_json::from_str(&text).map_err(|why| format!("{} does not read: {why}", path.display()))?;

  validate(&members).map_err(|why| format!("{}: {why}", path.display()))?;

  Ok(members)
}

/// The rules every member is held to, and that no name stands twice.
pub fn validate(members: &[Member]) -> Result<(), String> {
  let name_pattern = regex(&REGISTRY.name_pattern)?;
  let model_pattern = regex(&REGISTRY.model_pattern)?;
  let key_file_pattern = regex(&REGISTRY.key_file_pattern)?;
  let mut named: BTreeSet<&str> = BTreeSet::new();

  for member in members {
    let who = if member.name.is_empty() {
      "a member with no name".to_owned()
    } else {
      format!("the member {}", member.name)
    };

    if !model_pattern.is_match(&member.name) {
      return Err(format!("{who} does not match the model pattern"));
    }
    if !named.insert(member.name.as_str()) {
      return Err(format!("{who} stands twice"));
    }
    if !PROVIDERS.contains(&member.provider.as_str()) {
      return Err(format!("{who} names the provider {}, which is no member provider", member.provider));
    }
    if !model_pattern.is_match(&member.model) {
      return Err(format!("{who} names a model that does not match the model pattern"));
    }
    if !key_file_pattern.is_match(&member.key_file) {
      return Err(format!("{who} names a key file that does not match the key-file pattern"));
    }
    if let Some(road) = &member.road
      && !name_pattern.is_match(road)
    {
      return Err(format!("{who} names a road that does not match the name pattern"));
    }
    if member.label.as_deref().is_some_and(|label| label.chars().count() > REGISTRY.label_most) {
      return Err(format!("{who} names a label longer than {} characters", REGISTRY.label_most));
    }
    if let Some(base) = &member.api_base
      && Url::parse(base).is_err()
    {
      return Err(format!("{who} names an API base that is no address"));
    }
    if let Some(context) = member.context_tokens
      && !REGISTRY.context_tokens.holds(context)
    {
      return Err(format!("{who} names a context window outside the registry's bounds"));
    }
    if let Some(answer) = member.answer_tokens
      && !REGISTRY.answer_tokens.holds(answer)
    {
      return Err(format!("{who} names an answer ceiling outside the registry's bounds"));
    }
    if let (Some(context), Some(answer)) = (member.context_tokens, member.answer_tokens)
      && answer >= context
    {
      return Err(format!("{who} names an answer ceiling that leaves no room in its context window"));
    }
  }

  Ok(())
}

/// The members of one road, each with the window, the answer ceiling and the thinking a run on it takes.
/// A member of the road that leaves one of the three out is refused rather than sized at a guess.
pub fn for_road<'a>(members: &'a [Member], road: &str) -> Result<Vec<RoadMember<'a>>, String> {
  members
    .iter()
    .filter(|member| member.road.as_deref() == Some(road))
    .map(|member| {
      let context_tokens = member
        .context_tokens
        .ok_or_else(|| format!("the member {} of {road} names no context window", member.name))?;
      let answer_tokens = member
        .answer_tokens
        .ok_or_else(|| format!("the member {} of {road} names no answer ceiling", member.name))?;
      let thinking = member
        .thinking
        .ok_or_else(|| format!("the member {} of {road} names no thinking default", member.name))?;

      Ok(RoadMember {
        member,
        context_tokens,
        answer_tokens,
        thinking,
      })
    })
    .collect()
}

fn regex(pattern: &str) -> Result<Regex, String> {
  Regex::new(pattern).map_err(|why| format!("the registry's pattern {pattern} does not read: {why}"))
}

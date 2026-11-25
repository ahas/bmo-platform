//! Agency's procedures as resources, served by the `procedures` tool (docs/tools.md, 「The procedures」): one
//! document each under the supervisor's procedures folder of the tree the tool is placed in, listed by name and
//! read held to the verbs the road of the turn's room performs, which agency's server asks the consumer that
//! holds the rooms. A request reads its turn afresh from the session's record, and a turn a supervisor's reset
//! overtook is listed none and refused its read.

use std::path::{Path, PathBuf};

use regex::Regex;
use serde_json::{Value, json};
use sqlx::Row;
use tool_call::{Answer, Asked, Control, Handed, Session, Turn};

/// The address every procedure stands under.
pub const PROCEDURE_PREFIX: &str = "bmo://procedure/";

/// The route of agency's server the verbs of a room's road are asked at.
pub const ROAD_VERBS_PATH: &str = "/road-verbs";

/// The variable naming the token agency's server is asked under, which a refusal names.
const CONTROL_TOKEN: &str = "AGENCY_CONTROL_TOKEN";

/// A procedure's address by its name.
pub fn procedure_uri(name: &str) -> String {
  format!("{PROCEDURE_PREFIX}{name}")
}

/// The folder the procedure documents stand in, in the tree a reader stands in.
pub fn procedures_dir(tree: &Path) -> PathBuf {
  tree.join("prompts").join("agents").join("chat").join("procedures")
}

/// Every procedure by the name a session names it, which is its file's own without the extension.
pub fn procedure_names(tree: &Path) -> Vec<String> {
  let mut names: Vec<String> = std::fs::read_dir(procedures_dir(tree))
    .map(|entries| {
      entries
        .filter_map(Result::ok)
        .filter_map(|entry| entry.file_name().to_string_lossy().strip_suffix(".md").map(str::to_owned))
        .collect()
    })
    .unwrap_or_default();

  names.sort();
  names
}

/// A procedure document as it is written, or why it cannot be read: a name that is no procedure is refused
/// with the names there are rather than read.
pub fn written(tree: &Path, name: &str) -> Result<String, String> {
  let names = procedure_names(tree);

  if !names.iter().any(|one| one == name) {
    return Err(format!("no procedure named \"{name}\"; there are: {}", names.join(", ")));
  }

  std::fs::read_to_string(procedures_dir(tree).join(format!("{name}.md")))
    .map_err(|why| format!("the procedure {name} could not be read: {why}"))
}

/// A procedure document's text as a room whose road performs `can` may use it, or whole where it is read
/// outside any room; its trailing blank lines dropped either way.
pub fn as_used(text: &str, can: Option<&[String]>) -> String {
  match can {
    Some(can) => filter_procedure(text, can).trim_end().to_owned(),
    None => text.trim_end().to_owned(),
  }
}

// The procedure format's blocks are read here line by line, not by a markdown reader: a mark binds to the line it
// ends and to the lines indented under it, a level-two heading's to its whole section, and every line kept is
// kept byte for byte. That is the procedure format's own syntax laid over markdown, which a markdown reader's
// tree of blocks does not say.

// The mark, and the words around it that are never printed: ` [CAP:sticker]`.
fn capability_mark() -> Regex {
  Regex::new(r"\s*\[CAP:([A-Za-z,]+)\]\s*$").expect("the capability mark is a pattern")
}

// The verbs one line's mark names.
fn mark_of(mark: &Regex, line: &str) -> Vec<String> {
  mark
    .captures(line)
    .map(|found| found[1].split(',').filter(|word| !word.is_empty()).map(str::to_owned).collect())
    .unwrap_or_default()
}

fn indent_of(line: &str) -> usize {
  line.len() - line.trim_start().len()
}

// Where the block that starts at `start` ends: a heading's whole section, or a line and everything indented
// under it.
fn block_end(lines: &[&str], start: usize, limit: usize) -> usize {
  if lines[start].starts_with("## ") {
    return lines
      .iter()
      .enumerate()
      .skip(start + 1)
      .find(|(_, line)| line.starts_with("## "))
      .map_or(limit, |(next, _)| next.min(limit));
  }

  let indent = indent_of(lines[start]);
  let mut end = start + 1;

  while end < limit && !lines[end].trim().is_empty() && indent_of(lines[end]) > indent {
    end += 1;
  }

  end
}

// Where a block's own lines end: a heading owns its line alone; any other block its line and the lines it
// wraps onto, down to the first item written under it.
fn own_end(lines: &[&str], start: usize, limit: usize) -> usize {
  if lines[start].starts_with("## ") {
    return start + 1;
  }

  lines
    .iter()
    .enumerate()
    .skip(start + 1)
    .find(|(_, line)| line.trim_start().starts_with("- "))
    .map_or(limit, |(item, _)| item.min(limit))
}

fn filter_range(mark: &Regex, lines: &[&str], from: usize, to: usize, can: &[String]) -> Vec<String> {
  let mut kept = Vec::new();
  let mut index = from;

  while index < to {
    let line = lines[index];

    if line.trim().is_empty() {
      kept.push(line.to_owned());
      index += 1;
      continue;
    }

    let end = block_end(lines, index, to);
    let own = own_end(lines, index, end);
    let marks: Vec<String> = lines[index..own].iter().flat_map(|one| mark_of(mark, one)).collect();

    if marks.iter().all(|verb| can.contains(verb)) {
      kept.extend(lines[index..own].iter().map(|one| mark.replace(one, "").into_owned()));
      kept.extend(filter_range(mark, lines, own, end, can));
    }

    index = end;
  }

  kept
}

/// The document as a room whose road performs `can` reads it: the marked parts it cannot use gone, and the
/// marks themselves gone from what is left.
pub fn filter_procedure(text: &str, can: &[String]) -> String {
  let mark = capability_mark();
  let lines: Vec<&str> = text.split('\n').collect();

  format!("{}\n", filter_range(&mark, &lines, 0, lines.len(), can).join("\n").trim_end())
}

/// Agency's server as the tool reaches it (docs/tools.md, 「The procedures」).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Server {
  pub control: Control,
}

impl Server {
  /// The server the data root's `env/agency.env` describes, each variable this process holds standing first.
  pub fn of(data_root: &Path) -> Self {
    Self::read(data_root, |name| std::env::var(name).ok())
  }

  /// The server the data root's `env/agency.env` describes, each variable `variable` answers standing first.
  pub fn read(data_root: &Path, variable: impl Fn(&str) -> Option<String>) -> Self {
    Self {
      control: Control::read(data_root, variable),
    }
  }

  /// The verbs the road a room is answered on performs, `via` naming the road where the turn names one, as
  /// agency's server answers them; why not, where it was not asked or did not answer.
  pub async fn road_can(&self, chat: i64, via: Option<&str>) -> Result<Vec<String>, String> {
    let Some(token) = &self.control.token else {
      return Err(format!(
        "the verbs of the room's road cannot be asked: no {CONTROL_TOKEN} is named for agency's server"
      ));
    };
    let mut query = vec![("chat", chat.to_string())];

    query.extend(via.map(|via| ("via", via.to_owned())));

    let address =
      reqwest::Url::parse_with_params(&format!("{}{ROAD_VERBS_PATH}", self.control.base()), &query)
        .map_err(|why| format!("the address of agency's server does not read: {why}"))?;
    let answered = reqwest::Client::new()
      .get(address)
      .bearer_auth(token)
      .send()
      .await
      .map_err(|why| format!("agency's server could not be asked the verbs of the room's road: {why}"))?;
    let status = answered.status();
    let said: Value = answered.json().await.unwrap_or(Value::Null);

    if !status.is_success() {
      let why = said.get("error").and_then(Value::as_str).unwrap_or("no reason given");

      return Err(format!("agency's server refused the verbs of the room's road ({status}): {why}"));
    }

    said
      .get("can")
      .and_then(Value::as_array)
      .map(|verbs| verbs.iter().filter_map(Value::as_str).map(str::to_owned).collect())
      .ok_or_else(|| format!("agency's server answered {ROAD_VERBS_PATH} with no verbs"))
  }
}

/// The refusal of a turn a supervisor's reset overtook, read out of the store the record names; none for a turn
/// naming no agent or no moment, or where no reset point can be read.
pub async fn overtaken(session: &Session) -> Option<String> {
  let turn = Turn::of(session);
  let (Some(agent), Some(began)) = (turn.agent, turn.began) else {
    return None;
  };

  if session.archive().trim().is_empty() {
    return None;
  }

  let mut read = store::open_read(session.archive()).await.ok()?;
  let row = sqlx::query(r#"SELECT "value" FROM "settings" WHERE "key" = $1"#)
    .bind(store::reset_point::key(&agent))
    .fetch_optional(&mut read)
    .await
    .ok()??;
  let point = row.try_get::<String, _>("value").ok()?.trim().parse().ok();

  store::reset_point::overtakes(point, began).then(|| {
    format!(
      "{agent} was reset after this turn began, so nothing the turn does is kept any more: end the turn without another call"
    )
  })
}

/// Every procedure in the tree, as the protocol lists a resource; none for a turn a reset overtook.
pub async fn listed(tree: &Path, session: &Session) -> Vec<Value> {
  if overtaken(session).await.is_some() {
    return Vec::new();
  }

  procedure_names(tree)
    .iter()
    .map(|name| json!({ "uri": procedure_uri(name), "name": name, "mimeType": "text/markdown" }))
    .collect()
}

/// One procedure read from the tree for the session's turn: an address that is no procedure's refused before
/// the server is asked anything, and the document held to the verbs the turn room's road performs where the
/// turn has a room.
pub async fn read_in(tree: &Path, session: &Session, server: &Server, uri: &str) -> Result<Value, String> {
  if let Some(refusal) = overtaken(session).await {
    return Err(refusal);
  }

  let name = uri
    .strip_prefix(PROCEDURE_PREFIX)
    .ok_or_else(|| format!("there is no resource {uri}"))?;
  let text = written(tree, name)?;
  let turn = Turn::of(session);
  let can = match turn.chat {
    Some(chat) => Some(server.road_can(chat, turn.via.as_deref()).await?),
    None => None,
  };

  Ok(
    json!({ "contents": [{ "uri": uri, "mimeType": "text/markdown", "text": as_used(&text, can.as_deref()) }] }),
  )
}

/// One request answered: a listing, a read, or a call refused as no entry this tool holds, out of the tree the
/// tool is placed in.
pub async fn answer(handed: Handed) -> Result<Value, String> {
  let here = std::env::current_dir().unwrap_or_default();
  let tree = tool_call::prompts_tree(&here);
  let session = handed.session;

  match handed.asked {
    Asked::ResourcesList => Ok(json!({ "resources": listed(&tree, &session).await })),
    Asked::ResourcesRead { uri } => Ok(
      read_in(&tree, &session, &Server::of(&session.data_root()), &uri)
        .await
        .unwrap_or_else(|why| json!({ "error": why })),
    ),
    Asked::Call { entry, .. } => {
      serde_json::to_value(Answer::failure(format!("this executable holds no entry {entry}")))
        .map_err(|why| format!("the answer could not be written: {why}"))
    }
    Asked::Other => Err("this executable answers resource requests alone".to_owned()),
  }
}

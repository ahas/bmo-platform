//! A supervisor's system prompt, composed out of the agent documents exactly as a turn sends it
//! (docs/harness-machine.md, 「A supervisor's system prompt」; docs/supervisors.md, 「What a turn is
//! given」; docs/prompt-layers.md): its harness — the base prompt it chooses, else the platform's harness opening
//! on a program that takes it — the platform's rules where it is the platform administrator's, its
//! organisation's instructions where it has any, the role — the self-preservation rulebook, then the pieces its
//! settings list — and its persona where the chain keeps one, joined by a blank line.

use std::collections::BTreeMap;
use std::io;
use std::path::{Component, Path, PathBuf};
use std::sync::LazyLock;

use policy::base_prompts::BASE_PROMPTS;
use policy::prompt_layers::PROMPT_LAYERS;
use policy::supervisors::SUPERVISORS;
use regex::{Captures, Regex};
use serde::Deserialize;

// The rulebook every agent that acts is given, which no setting leaves out.
const SELF_PRESERVATION: &str = "self-preservation";

static VALUE: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"\{\{(\w+)\}\}").expect("the value mark reads"));

/// A template with each `{{name}}` it carries filled in where a value is given, and any other left
/// as written.
pub fn render(template: &str, values: &BTreeMap<&str, String>) -> String {
  VALUE
    .replace_all(template, |found: &Captures| {
      values.get(&found[1]).cloned().unwrap_or_else(|| found[0].to_owned())
    })
    .into_owned()
}

/// What composing a supervisor's prompt reads of its settings.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Composition {
  /// The persona its settings hold, or none for its own file.
  pub persona: Option<String>,
}

/// What stands between a supervisor's harness and its role, and whether its persona closes the prompt
/// (docs/prompt-layers.md). The caller resolves them: whose the supervisor is, and what the delegation chain
/// says for the person it serves.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Layers {
  /// Whether the platform's rules stand after the harness: a supervisor of no account, or of the platform
  /// administrator's.
  pub platform: bool,
  /// The organisation's instructions as composed, or none.
  pub organisation: Option<String>,
  /// Whether the chain chose no persona, so none closes the prompt.
  pub persona_none: bool,
  /// The person's own instructions, where the chain lets them give any; each agent takes its own part of them.
  #[serde(default)]
  pub personal: Personal,
}

/// The instructions a person gives their own agents: one text for every agent of theirs, and one for a single
/// agent by its name (docs/prompt-layers.md, 「The person's instructions」).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Personal {
  #[serde(default)]
  pub all: Option<String>,
  #[serde(default)]
  pub agents: BTreeMap<String, String>,
}

impl Personal {
  /// What an agent named `agent` is given of them: the text for every agent, then its own, under their header and
  /// closed by their end line; none where neither says anything.
  pub fn of(&self, agent: &str) -> Option<String> {
    let said: Vec<&str> = [self.all.as_deref(), self.agents.get(agent).map(String::as_str)]
      .into_iter()
      .flatten()
      .map(str::trim)
      .filter(|text| !text.is_empty())
      .collect();

    if said.is_empty() {
      return None;
    }

    let words = &PROMPT_LAYERS.personal;

    Some(format!("{}\n\n{}\n\n{}", words.header, said.join("\n\n"), words.end))
  }
}

/// The organisation's instructions as a prompt carries them: every link's text, the outermost first, under
/// their header and closed by their end line; none where no link says any.
pub fn organisation(texts: &[String]) -> Option<String> {
  let said: Vec<&str> = texts.iter().map(|text| text.trim()).filter(|text| !text.is_empty()).collect();

  if said.is_empty() {
    return None;
  }

  let words = &PROMPT_LAYERS.organisation;

  Some(format!("{}\n\n{}\n\n{}", words.header, said.join("\n\n"), words.end))
}

/// The base prompt a supervisor chooses, and the model it is pinned to, which the prompt may name.
#[derive(Clone, Debug, Deserialize)]
pub struct Base {
  pub text: String,
  pub model: String,
}

/// The supervisor a prompt is composed for: its name, which its working folder is kept under; its label and
/// its call names, which its frame tells it it is called by; and the prompt folder its agent row names under
/// `prompts/agents/`, none where the row names none.
#[derive(Clone, Copy, Debug)]
pub struct Supervisor<'a> {
  pub name: &'a str,
  pub label: Option<&'a str>,
  pub call_names: &'a [String],
  pub prompts: Option<&'a str>,
}

impl Supervisor<'_> {
  /// The name it stands as in the rooms: its label, else its registration name.
  fn standing_name(&self) -> &str {
    self.label.map(str::trim).filter(|label| !label.is_empty()).unwrap_or(self.name)
  }

  /// The names a person calls it by, as the frame lists them: its call names, else the name it stands as.
  fn called_by(&self) -> String {
    if self.call_names.is_empty() {
      self.standing_name().to_owned()
    } else {
      self.call_names.join(", ")
    }
  }
}

/// The roots a prompt is composed out of: the documents' tree, and the data root a supervisor's folder
/// stands under.
#[derive(Clone, Debug)]
pub struct Roots {
  pub assets: PathBuf,
  pub data: PathBuf,
}

impl Roots {
  fn required(path: PathBuf) -> io::Result<String> {
    std::fs::read_to_string(&path)
      .map(|text| text.trim().to_owned())
      .map_err(|error| io::Error::new(error.kind(), format!("{}: {error}", path.display())))
  }

  /// One agent's prompt folder under the prompts, as a row names it: one plain name, never a path that
  /// could reach outside `prompts/agents/`.
  pub fn prompt_folder(&self, folder: &str) -> io::Result<PathBuf> {
    let mut parts = Path::new(folder).components();

    match (parts.next(), parts.next()) {
      (Some(Component::Normal(_)), None) => Ok(self.assets.join("prompts/agents").join(folder)),
      _ => Err(io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("a prompt folder is one plain name: {folder:?}"),
      )),
    }
  }

  /// A document of one agent's prompt folder.
  pub fn agent_file(&self, folder: &str, file: &str) -> io::Result<String> {
    Self::required(self.prompt_folder(folder)?.join(file))
  }

  /// A rulebook as an agent is given it: named, so the agent can say which rule it applied.
  pub fn rulebook(&self, name: &str) -> io::Result<String> {
    let text = Self::required(self.assets.join("prompts/rules").join(format!("{name}.md")))?;

    Ok(format!("RULEBOOK prompts/rules/{name}.md:\n{text}"))
  }

  /// One of the platform's rulebooks, as the platform administrator's agents are given it: named, as every
  /// rulebook is.
  pub fn platform_rulebook(&self, name: &str) -> io::Result<String> {
    let folder = &PROMPT_LAYERS.platform_folder;
    let text = Self::required(self.assets.join("prompts").join(folder).join(format!("{name}.md")))?;

    Ok(format!("RULEBOOK prompts/{folder}/{name}.md:\n{text}"))
  }

  /// The ABSOLUTE RULES, bare, under their own heading line.
  pub fn absolute_rules(&self) -> io::Result<String> {
    Self::required(self.assets.join("prompts/rules/absolute-rules.md"))
  }

  /// The platform's harness opening, which a program running on Claude Code is handed as its harness.
  pub fn harness(&self) -> io::Result<String> {
    Self::required(self.assets.join("prompts/harness.md"))
  }

  /// The persona a supervisor is sent: its settings' text where they hold one, its prompt folder's file
  /// otherwise, and none for a supervisor that holds neither, as one added on the console starts.
  pub fn persona(&self, prompts: Option<&str>, held: Option<&str>) -> io::Result<String> {
    if let Some(text) = held {
      return Ok(text.to_owned());
    }

    let Some(folder) = prompts else {
      return Ok(String::new());
    };

    if self.prompt_folder(folder)?.join("persona.md").exists() {
      self.agent_file(folder, "persona.md")
    } else {
      Ok(String::new())
    }
  }
}

/// The folder every supervisor's home stands in under a data root.
pub fn homes(data: &Path) -> PathBuf {
  data.join("supervisor-homes")
}

/// The folder a supervisor stands in: its own under the data root, kept across deployments, where its shell
/// starts and the files it makes for itself go.
pub fn folder(roots: &Roots, agent: &str) -> PathBuf {
  homes(&roots.data).join(agent)
}

/// What the frame says a supervisor can do: the chat line's tools, which every conversational agent holds, and
/// its folder. The folder is named by its path to a supervisor the
/// platform's rules reach alone, since the path stands under the platform's own data root; any other is told it
/// in the prompt layers' words (docs/prompt-layers.md).
fn tools_text(roots: &Roots, agent: &str, layers: &Layers) -> io::Result<String> {
  let template = roots.agent_file("pieces", "tools.md")?;
  let workplace = if layers.platform {
    folder(roots, agent).display().to_string()
  } else {
    PROMPT_LAYERS.workplace_unnamed.clone()
  };

  Ok(render(&template, &BTreeMap::from([("workplace", workplace)])))
}

/// What a base prompt's values read on this machine for one run: the run's folder and model, and the
/// machine's date, platform, system and shell.
pub fn base_value(name: &str, cwd: &Path, model: &str) -> Option<String> {
  Some(match name {
    "cwd" => cwd.display().to_string(),
    "date" => machine::local_date(),
    "platform" => std::env::consts::OS.to_owned(),
    "os_version" => machine::os_version(),
    "shell" => std::env::var_os("SHELL").map_or_else(
      || "sh".to_owned(),
      |shell| {
        Path::new(&shell)
          .file_name()
          .map(|name| name.to_string_lossy().into_owned())
          .unwrap_or_default()
      },
    ),
    "model" => model.to_owned(),
    _ => return None,
  })
}

/// What this machine says of itself for a base prompt's values.
pub mod machine {
  use std::ffi::CStr;

  /// Today's date on this machine, as `YYYY-MM-DD`.
  pub fn local_date() -> String {
    // SAFETY: time with a null pointer only answers the clock; localtime_r writes into the zeroed
    // struct handed to it and reads the zone the process was started in.
    unsafe {
      let now = libc::time(std::ptr::null_mut());
      let mut local: libc::tm = std::mem::zeroed();

      libc::localtime_r(&now, &mut local);
      format!("{:04}-{:02}-{:02}", local.tm_year + 1900, local.tm_mon + 1, local.tm_mday)
    }
  }

  /// The operating system's name and its kernel's release, as uname gives them.
  pub fn os_version() -> String {
    // SAFETY: uname fills the zeroed struct handed to it, each field a terminated string.
    unsafe {
      let mut named: libc::utsname = std::mem::zeroed();

      if libc::uname(&mut named) != 0 {
        return String::new();
      }

      let field = |chars: &[libc::c_char]| CStr::from_ptr(chars.as_ptr()).to_string_lossy().into_owned();

      format!("{} {}", field(&named.sysname), field(&named.release))
    }
  }
}

/// The base prompt filled in for a supervisor's turn: the folder it stands in and the model it
/// is pinned to.
pub fn filled_base(roots: &Roots, agent: &str, base: &Base) -> String {
  let cwd = folder(roots, agent);
  let values = BASE_PROMPTS
    .values
    .iter()
    .filter_map(|name| Some((name.as_str(), base_value(name, &cwd, &base.model)?)))
    .collect();

  render(&base.text, &values).trim().to_owned()
}

pub fn piece(
  roots: &Roots,
  supervisor: Supervisor,
  piece: &str,
  (settings, layers): (&Composition, &Layers),
) -> io::Result<String> {
  match piece {
    "absolute-rules" => roots.absolute_rules(),
    "frame" => {
      let frame = roots.agent_file("pieces", "frame.md")?;
      let values = BTreeMap::from([
        ("label", supervisor.standing_name().to_owned()),
        ("call_names", supervisor.called_by()),
        ("tools", tools_text(roots, supervisor.name, layers)?),
      ]);

      Ok(render(&frame, &values))
    }
    "turn" => roots.agent_file("pieces", "turn.md"),
    "agent-words" => roots.rulebook("agent-words"),
    "persona" => roots.persona(supervisor.prompts, settings.persona.as_deref()),
    other => Err(io::Error::other(format!(
      "the shared policy names a piece this composer has no text for: {other}"
    ))),
  }
}

/// The whole system prompt, its layers each whole in their order (docs/prompt-layers.md): its harness; the
/// platform's rules where `layers` attaches them; its organisation's instructions where it has any; every
/// supervisor's role whole — the rulebook against destroying the system and the role's parts in their fixed
/// order — then the instructions of the person it serves where they give any, and its persona, unless the chain
/// chose none. The harness is the base prompt it chooses; where it
/// chooses none, the platform's harness opening when `harnessed` says its program takes one, and none for a
/// program whose own prompt is its harness.
pub fn system(
  roots: &Roots,
  supervisor: Supervisor,
  settings: &Composition,
  base: Option<&Base>,
  harnessed: bool,
  layers: &Layers,
) -> io::Result<String> {
  let mut parts = vec![];

  match base {
    Some(base) => parts.push(filled_base(roots, supervisor.name, base)),
    None if harnessed => parts.push(roots.harness()?),
    None => {}
  }

  if layers.platform {
    for name in &SUPERVISORS.pieces_platform {
      parts.push(roots.platform_rulebook(name)?);
    }
  }

  parts.extend(layers.organisation.clone());
  parts.push(roots.rulebook(SELF_PRESERVATION)?);

  for name in &SUPERVISORS.pieces_role {
    parts.extend(Some(piece(roots, supervisor, name, (settings, layers))?).filter(|text| !text.is_empty()));
  }

  parts.extend(layers.personal.of(supervisor.name));

  if !layers.persona_none {
    parts.extend(
      Some(piece(roots, supervisor, "persona", (settings, layers))?).filter(|text| !text.is_empty()),
    );
  }

  Ok(parts.join("\n\n"))
}

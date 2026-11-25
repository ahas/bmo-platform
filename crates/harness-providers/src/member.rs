//! The mechanism every provider whose runs are Claude Code on agency's gateway shares: the members of its
//! road, their key files, the home each model keeps its records under, whether a standing program is one of
//! its own, and the records a reset takes (docs/own-llm.md, 「The gateway's members」, docs/harness-machine.md,
//! 「The members」).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use harness_machine::members::{self, Member};

use crate::claude_code::ClaudeCode;
use crate::registered::Registered;
use crate::{Machine, Reason};

/// One member of a road, with what a run on it is sized to.
#[derive(Clone, Debug)]
pub struct Road {
  pub member: Member,
  pub context_tokens: i64,
  pub answer_tokens: i64,
  pub thinking: bool,
}

/// The members the deployment holds for one road. A file that does not read, or a member that leaves out what
/// a run needs, is the reason the road cannot answer.
pub fn road_members(machine: &Machine, road: &str) -> Result<Vec<Road>, String> {
  let members = members::read(&members::path(machine.data_root()))?;

  Ok(
    members::for_road(&members, road)?
      .into_iter()
      .map(|held| Road {
        member: held.member.clone(),
        context_tokens: held.context_tokens,
        answer_tokens: held.answer_tokens,
        thinking: held.thinking,
      })
      .collect(),
  )
}

/// The member of a road one name holds, where it holds one.
pub fn member_named(machine: &Machine, road: &str, name: &str) -> Result<Option<Member>, String> {
  Ok(
    road_members(machine, road)?
      .into_iter()
      .find(|held| held.member.name == name)
      .map(|held| held.member),
  )
}

/// Whether the member of a road one name holds costs nothing: its own `free` where it says, else what
/// `unstated`, the road's provider's reading, reads off it. A name the road holds no member for, and a file
/// that does not read, cost.
pub fn free(machine: &Machine, road: &str, name: &str, unstated: impl FnOnce(&Member) -> bool) -> bool {
  member_named(machine, road, name)
    .ok()
    .flatten()
    .is_some_and(|member| member.free.unwrap_or_else(|| unstated(&member)))
}

/// Why a road cannot answer on this machine now, or nothing where it can: the gateway's key file, at least
/// one member of the road, and each member's own key file, all private and not empty.
pub fn missing(machine: &Machine, road: &str, code: &str, text: &str) -> Option<Reason> {
  if !usable(&Registered::key_file(machine)) {
    return Some(Reason::new(code, text));
  }

  let held = match road_members(machine, road) {
    Ok(held) => held,
    Err(_) => return Some(Reason::new(code, text)),
  };

  if held.is_empty() || held.iter().any(|one| !usable(&key_file(machine, &one.member))) {
    return Some(Reason::new(code, text));
  }

  None
}

/// The key file a member names, under the data root's `env/`.
pub fn key_file(machine: &Machine, member: &Member) -> PathBuf {
  machine.data_root().join("env").join(&member.key_file)
}

/// The folder every home of one road stands under.
pub fn homes(machine: &Machine, road: &str) -> PathBuf {
  machine.data_root().join("claude-roads").join(road)
}

/// The home Claude Code is handed for a run on one member of a road.
pub fn home(machine: &Machine, road: &str, model: &str) -> PathBuf {
  homes(machine, road).join(model)
}

/// Whether a standing program is Claude Code on one road: its configuration home is under the road's homes.
pub fn owns(machine: &Machine, args: &[String], env: &BTreeMap<String, String>, road: &str) -> bool {
  let homes = format!("{}/", homes(machine, road).display());

  args.first().is_some_and(|program| program == ClaudeCode::PROGRAM)
    && env.get("CLAUDE_CONFIG_DIR").is_some_and(|dir| dir.starts_with(&homes))
}

/// The records one session of a member's road keeps in `cwd`, wherever they stand.
pub fn session_records(
  machine: &Machine,
  road: &str,
  cwd: &Path,
  session: &str,
  model: &str,
) -> Vec<PathBuf> {
  ClaudeCode::records_under(&home(machine, road, model), cwd, session)
}

/// The records every session of a road's homes keeps in `cwd`.
pub fn home_records(machine: &Machine, road: &str, cwd: &Path) -> Vec<PathBuf> {
  let Ok(homes) = std::fs::read_dir(homes(machine, road)) else {
    return vec![];
  };

  homes
    .flatten()
    .flat_map(|home| ClaudeCode::every_record_under(&home.path(), cwd))
    .collect()
}

/// The prompts a road's homes kept in a history of what was put in while it worked in `cwd`, taken out.
pub fn forget_prompts(
  machine: &Machine,
  road: &str,
  cwd: &Path,
  session: Option<&str>,
) -> Result<usize, String> {
  let Ok(homes) = std::fs::read_dir(homes(machine, road)) else {
    return Ok(0);
  };

  ClaudeCode::forget_prompts_in_each(homes.flatten().map(|home| home.path()), cwd, session)
}

/// Whether the gateway's own key file is usable: a file of its own, private to its owner, not empty.
fn usable(path: &Path) -> bool {
  use std::os::unix::fs::PermissionsExt;

  std::fs::metadata(path)
    .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o777 == 0o600 && meta.len() > 0)
}

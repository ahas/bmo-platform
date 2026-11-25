//! What is running on the machine, as agency's server reads it (docs/agent-control.md, 「Asking whether it
//! is alive」): a name is alive where systemd holds its unit, an open execution names it as its parent or
//! root, or herdr holds it as a pane's run — the union, since asking systemd alone about a run in a pane
//! reads it dead the moment it starts. Which jobs those names are, and the workers they hold, are the job
//! unit's to read off its own rows (docs/api.md, 「Workers」). "Nothing is running" and "nobody could say"
//! are kept apart throughout: a machine nobody could ask is never read as one with nothing running.

use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Stdio;

use harness_providers::Machine;
use herdr::picture::Watch;
use sea_orm::DatabaseConnection;
use serde_json::Value;
use store::all;

use super::settings;
use crate::Refusal;

/// The settings key a cancellation standing over a name is kept under.
pub const EXECUTION_CANCEL_PREFIX: &str = "execution_cancel:";

/// What the server reads of what runs: the store's open rows, the machine's systemd and the data
/// root its executions keep their outcomes under, and herdr's picture where the process watches one.
#[derive(Clone)]
pub struct Running {
  db: DatabaseConnection,
  data_root: PathBuf,
  systemctl: Option<PathBuf>,
  herdr: Option<Watch>,
}

impl Running {
  pub fn new(db: DatabaseConnection, machine: &Machine, herdr: Option<Watch>) -> Self {
    Self {
      db,
      data_root: machine.data_root().to_path_buf(),
      systemctl: machine.program("systemctl"),
      herdr,
    }
  }

  /// Which of these units the machine's systemd holds active, asked of it alone in one call: it prints
  /// one line per unit in the order asked. None where it could not be asked or gave fewer lines than it
  /// was asked about.
  pub async fn systemd(&self, units: &[String]) -> Option<HashSet<String>> {
    let mut command = std::process::Command::new(self.systemctl.as_ref()?);

    command
      .args(["--user", "is-active"])
      .args(units.iter().map(|unit| format!("{unit}.service")))
      .stdin(Stdio::null())
      .stderr(Stdio::null());
    harness_machine::model_use::with_user_bus(&mut command);

    let said = tokio::process::Command::from(command).output().await.ok()?;
    let lines: Vec<String> = String::from_utf8_lossy(&said.stdout)
      .lines()
      .map(str::trim)
      .filter(|line| !line.is_empty())
      .map(str::to_owned)
      .collect();

    (lines.len() >= units.len()).then(|| {
      units
        .iter()
        .zip(&lines)
        .filter(|(_, state)| *state == "active")
        .map(|(unit, _)| unit.clone())
        .collect()
    })
  }

  // systemd's answer with whatever else holds a run joined to it: an open execution naming one as its
  // owner — unless it shares its parent's pane, its host has published how it ended, or a cancellation
  // stands over it — and a run herdr holds.
  async fn joined(&self, units: &[String], mut alive: HashSet<String>) -> Result<HashSet<String>, Refusal> {
    let rows = all(
      &self.db,
      r#"SELECT "body" FROM "agent_executions" WHERE ("body"::json ->> 'endedAt') IS NULL ORDER BY rowid DESC"#,
      vec![],
    )
    .await?;

    for row in rows {
      let Ok(body) = serde_json::from_str::<Value>(&row.try_get::<String>("", "body")?) else {
        continue;
      };
      let id = body.get("id").and_then(Value::as_str).unwrap_or_default();

      if body.get("sharedPane") == Some(&Value::Bool(true)) {
        continue;
      }

      if body.get("line").and_then(Value::as_str) == Some("coordinator")
        && self.data_root.join("executions").join(id).join("outcome.json").exists()
      {
        continue;
      }

      for owner in ["parent", "root"].iter().filter_map(|key| body.get(*key).and_then(Value::as_str)) {
        if owner.is_empty() || !units.iter().any(|unit| unit == owner) {
          continue;
        }

        if self.cancelled(&[owner, id]).await? {
          continue;
        }

        alive.insert(owner.to_owned());
      }
    }

    if let Some(herdr) = &self.herdr {
      let held = herdr.names();

      alive.extend(units.iter().filter(|unit| held.contains(*unit)).cloned());
    }

    Ok(alive)
  }

  // Whether a standing cancellation covers any of these names.
  async fn cancelled(&self, names: &[&str]) -> Result<bool, Refusal> {
    for name in names.iter().filter(|name| !name.is_empty()) {
      if settings::get(&self.db, &format!("{EXECUTION_CANCEL_PREFIX}{name}")).await?.is_some() {
        return Ok(true);
      }
    }

    Ok(false)
  }

  /// Which of these units still stand, or none where systemd gave no answer: for a caller to whom
  /// "nothing stands" and "nobody could say" call for different things.
  pub async fn answered(&self, units: &[String]) -> Result<Option<HashSet<String>>, Refusal> {
    if units.is_empty() {
      return Ok(Some(HashSet::new()));
    }

    match self.systemd(units).await {
      Some(alive) => self.joined(units, alive).await.map(Some),
      None => Ok(None),
    }
  }

  /// Which of these units still stand, systemd giving no answer read as none of its own.
  pub async fn active(&self, units: &[String]) -> Result<HashSet<String>, Refusal> {
    if units.is_empty() {
      return Ok(HashSet::new());
    }

    let alive = self.systemd(units).await.unwrap_or_default();

    self.joined(units, alive).await
  }
}

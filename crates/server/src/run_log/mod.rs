//! The run log (docs/run-records.md): one line for everything an agent did, written to the store as it
//! happens, so a run killed from outside leaves everything up to the kill. This writer is the one a job's
//! turns read out of a Claude Code program's record are logged by: the job's lines keep their detail
//! whole, are numbered in the job's own order, and leave out every credential they would carry.

pub mod codex;
pub mod json;
pub mod lines;
pub mod pane_calls;
pub mod prune;
pub mod secrets;
pub mod spans;

use std::path::Path;

use policy::deployment::DEPLOYMENT;
use sea_orm::DatabaseConnection;
use sha2::Digest;
use store::{one, run};

use self::codex::{CodexChanges, Waiting, codex_lines, rollout_lines, stream_paths};
use self::json::Json;
use self::lines::{Line, record_lines, stream_lines};
use self::pane_calls::PaneCalls;
use self::secrets::{MASKED_VALUE, mask_secrets, reads_secret_file};
use self::spans::{JobSpan, SpanTally};
use crate::rooms::Stays;
use crate::{Refusal, now_ms};

crate::room_stays!(Stays {
  table: "agent_log",
  stays: r#""job_id" IS NOT NULL"#,
  why: "a job's turns are logged under its room but belong to the job, which outlives the room",
});

/// What a tool's line may carry in its list preview.
pub const MAX_TOOL_CHARS: usize = 2000;
/// What a prompt's line may carry in its list preview.
pub const MAX_PROMPT_CHARS: usize = 4000;
/// What a job's line may carry in its list preview, its whole text kept as its detail.
pub const MAX_JOB_PREVIEW_CHARS: usize = 4000;

// A run writes as it goes and may never reach an end, so its agent's lines are pruned with the writing.
const PRUNE_EVERY_LINES: u64 = 200;

// The name a run's system prompt line goes under.
const SYSTEM_PROMPT_LINE: &str = "system";

/// Whose lines these are: the agent, the run they belong to, its room and its job.
#[derive(Clone, Debug)]
pub struct Target {
  pub agent: String,
  pub run: String,
  pub chat_id: Option<i64>,
  pub job_id: Option<i64>,
}

/// The writer of one run's lines.
pub struct RunLog {
  target: Target,
  tally: SpanTally,
  opened_secrets: std::collections::HashSet<String>,
  appended: u64,
  sequence: Option<i64>,
  source_event: i64,
  // The record a job's Codex run writes its changes into, once its stream has named its thread.
  codex: Option<CodexChanges>,
  // Whether the log keeps what each run was launched with whole, its prompts uncut.
  keeps_launch: bool,
  // The calls a pane program's record says began and has not yet said settled.
  pane_calls: PaneCalls,
  // Whether a job's lines were put under the agent its item names, read once before its first line.
  agent_named: bool,
}

/// What a line keeps of its text: the first `limit` characters, counted as the store counts them, and
/// whether it was cut.
pub fn preview(text: &str, limit: Option<usize>) -> (String, bool) {
  match limit.and_then(|limit| text.char_indices().nth(limit)) {
    Some((end, _)) => (text[..end].to_owned(), true),
    None => (text.to_owned(), false),
  }
}

impl RunLog {
  pub fn new(target: Target) -> Self {
    Self {
      target,
      tally: SpanTally::default(),
      opened_secrets: std::collections::HashSet::new(),
      appended: 0,
      sequence: None,
      source_event: 0,
      codex: None,
      keeps_launch: false,
      pane_calls: PaneCalls::default(),
      agent_named: false,
    }
  }

  /// The same log keeping what each run was launched with whole: a supervisor's turn, whose screen reads every
  /// prompt it was handed.
  pub fn keeping_launch(mut self) -> Self {
    self.keeps_launch = true;
    self
  }

  /// The run these lines are kept under.
  pub fn run(&self) -> &str {
    &self.target.run
  }

  /// The job these lines are a run of, where they are a job's.
  pub fn job(&self) -> Option<i64> {
    self.target.job_id
  }

  /// Where the job's lines left off read again before the next line: another writer of the same job has
  /// written its own lines meanwhile.
  pub fn resync(&mut self) {
    self.sequence = None;
  }

  /// One line written, answering its id; a line the store would not take is said in the machine's log
  /// and lost, which never ends the run.
  pub async fn write(&mut self, db: &DatabaseConnection, line: Line) -> Option<i64> {
    let mut kept = self.kept(db, &line).await;

    // A job's line numbered past one another writer of the same job filed meanwhile is filed once more after
    // reading where the job's lines left off (docs/run-records.md, 「Agent log」).
    if kept.is_err() && self.target.job_id.is_some() && self.sequence.is_some() {
      self.resync();
      kept = self.kept(db, &line).await;
    }

    match kept {
      Ok(id) => {
        self.appended += 1;

        if self.appended.is_multiple_of(PRUNE_EVERY_LINES)
          && let Err(why) = prune(db, &self.target.agent).await
        {
          eprintln!("{}: the log prune: {}", self.target.agent, why.detail);
        }

        Some(id)
      }
      Err(why) => {
        eprintln!("{}: a {} line: {}", self.target.agent, line.kind, why.detail);
        None
      }
    }
  }

  async fn kept(&mut self, db: &DatabaseConnection, line: &Line) -> Result<i64, Refusal> {
    let job_id = self.target.job_id;

    // A job's lines stand under the agent its item names, the one doing the work, not the checkout it runs in
    // (docs/run-records.md, 「Agent log」); a job that names none keeps the name it was opened under.
    if let (Some(job), false) = (job_id, self.agent_named) {
      let named = one(db, r#"SELECT "item_agent" FROM "jobs" WHERE "id" = $1"#, vec![job.into()])
        .await?
        .and_then(|row| row.try_get::<Option<String>>("", "item_agent").ok().flatten())
        .filter(|agent| !agent.trim().is_empty());

      if let Some(agent) = named {
        self.target.agent = agent;
      }

      self.agent_named = true;
    }
    let sequence = match job_id {
      None => None,
      Some(job) => {
        let last = match self.sequence {
          Some(last) => last,
          None => one(
            db,
            r#"SELECT COALESCE(MAX("sequence"), 0) AS "sequence" FROM "agent_log" WHERE "job_id" = $1"#,
            vec![job.into()],
          )
          .await?
          .map_or(Ok(0), |row| row.try_get::<i64>("", "sequence"))?,
        };

        self.sequence = Some(last + 1);
        Some(last + 1)
      }
    };
    let own = match line.kind {
      "tool" | "tool-result" => Some(MAX_TOOL_CHARS),
      "prompt" if !self.keeps_launch => Some(MAX_PROMPT_CHARS),
      _ => None,
    };
    let cap = if job_id.is_some() {
      Some(own.map_or(MAX_JOB_PREVIEW_CHARS, |own| own.min(MAX_JOB_PREVIEW_CHARS)))
    } else {
      own
    };
    let text = self.without_secrets(line);
    let (clipped, cut) = preview(&text, cap);
    let keeps_detail = job_id.is_some();
    let detail = (keeps_detail && cut).then(|| text.clone());
    let metadata = if keeps_detail { line.metadata.as_ref().map(Json::stringify) } else { None };
    let detail_length = detail.as_deref().unwrap_or(&clipped).chars().count() as i64;
    let metadata_length = metadata.as_deref().map_or(0, |held| held.chars().count() as i64);
    let row = one(
      db,
      r#"INSERT INTO "agent_log"
           ("agent", "run", "chat_id", "at", "kind", "name", "parent", "ref", "truncated", "text",
            "job_id", "sequence", "source", "source_event", "source_block", "source_id", "state",
            "upstream_truncated", "detail_complete", "detail_length", "metadata_length", "detail", "metadata")
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22, $23)
         RETURNING "id""#,
      vec![
        self.target.agent.clone().into(),
        self.target.run.clone().into(),
        self.target.chat_id.into(),
        now_ms().into(),
        line.kind.into(),
        line.name.clone().into(),
        line.parent.clone().into(),
        line.reference.clone().into(),
        i64::from(cut).into(),
        clipped.into(),
        job_id.into(),
        sequence.into(),
        Some(line.source.clone().unwrap_or_else(|| DEPLOYMENT.agent_key.clone())).into(),
        line.source_event.into(),
        line.source_block.into(),
        line.source_id.clone().into(),
        line.state.map(str::to_owned).into(),
        i64::from(line.upstream_truncated).into(),
        i64::from(keeps_detail || !cut).into(),
        detail_length.into(),
        metadata_length.into(),
        detail.into(),
        metadata.into(),
      ],
    )
    .await?;

    Ok(row.map_or(Ok(0), |row| row.try_get::<i64>("", "id"))?)
  }

  // A line's text with no credential left in it: a tool's arguments and what a tool handed back are the
  // two that carry one, and a result of a call that opened a file of credentials keeps nothing at all.
  fn without_secrets(&mut self, line: &Line) -> String {
    if line.kind == "tool" {
      if let Some(reference) = &line.reference
        && reads_secret_file(&line.text)
      {
        self.opened_secrets.insert(reference.clone());
      }

      return mask_secrets(&line.text);
    }

    if line.kind != "tool-result" {
      return line.text.clone();
    }

    match &line.reference {
      Some(reference) if self.opened_secrets.contains(reference) => MASKED_VALUE.to_owned(),
      _ => mask_secrets(&line.text),
    }
  }

  /// The record lines a turn wrote, each read as the stream event it is — or, out of a Codex rollout, as the
  /// response item it is — and filed under the road it went out on; a line that is not JSON is no event. The
  /// lines written, for whoever watches the run go.
  pub async fn feed(&mut self, db: &DatabaseConnection, records: &[String], source: &str) -> Vec<Line> {
    let mut written = vec![];

    for record in records {
      let trimmed = record.trim();

      if !trimmed.starts_with('{') {
        continue;
      }

      let Some(event) = Json::parse(trimmed) else { continue };

      // Only what the run said and did is a stream's event; the program's own bookkeeping is not. A pane
      // program's record is its own shape: its turn's progress and the answer it ended on are filed as the
      // lines they are, for the room that reads the run as it goes, and a call it says began or settled as
      // the call's own line beside them.
      let lines = match event.get("type").and_then(Json::as_str) {
        Some("assistant" | "user") => {
          self.tally.see(&event);
          record_lines(&event)
        }
        Some("response_item") => rollout_lines(&event),
        None => match event.get("kind").and_then(Json::as_str) {
          Some(kind @ ("progress" | "answer")) => {
            let kind: &'static str = if kind == "progress" { "progress" } else { "answer" };
            let calls = if kind == "progress" { self.pane_calls.lines(&event) } else { vec![] };

            std::iter::once(Line {
              kind,
              text: trimmed.to_owned(),
              ..Line::default()
            })
            .chain(calls)
            .collect()
          }
          _ => continue,
        },
        _ => continue,
      };

      self.source_event += 1;

      for line in lines {
        let line = Line {
          source: Some(source.to_owned()),
          source_event: Some(self.source_event),
          ..line
        };

        self.write(db, line.clone()).await;
        written.push(line);
      }
    }

    written
  }

  /// The start of a run apart (docs/agents.md, 「Runs apart」), which its record says nowhere: the model it went
  /// out on, filed under the road it went out on.
  pub async fn apart_started(&mut self, db: &DatabaseConnection, source: &str, model: &str) {
    let line = Line {
      kind: "start",
      text: model.to_owned(),
      source: Some(source.to_owned()),
      ..Line::default()
    };

    self.write(db, line).await;
  }

  /// The end of a run apart, which its record says nowhere either: its answer where it gave one, and otherwise
  /// what ended it, filed under the road it went out on — the line a run's spend is shown against.
  pub async fn apart_ended(&mut self, db: &DatabaseConnection, source: &str, answer: Result<&str, &str>) {
    let (text, state) = match answer {
      Ok(text) if text.trim().is_empty() => ("done".to_owned(), "succeeded"),
      Ok(text) => (format!("done\n{}", text.trim()), "succeeded"),
      Err(why) => (format!("error\n{why}"), "failed"),
    };
    let line = Line {
      kind: "end",
      text,
      state: Some(state),
      source: Some(source.to_owned()),
      ..Line::default()
    };

    self.write(db, line).await;
  }

  /// What a run was handed as it was launched: its system prompt where it differs from the last one the agent's
  /// log holds, read off the file the program reads, its prompt, and what went into that prompt without the
  /// session asking for anything, in the order it entered it, by where it came from.
  pub async fn launched(
    &mut self,
    db: &DatabaseConnection,
    system: Option<&Path>,
    prompt: &str,
    context: &[(String, String)],
  ) {
    let context: Vec<(String, String, Option<&'static str>)> =
      context.iter().map(|(source, text)| (source.clone(), text.clone(), None)).collect();

    self.launched_holding(db, system, prompt, &context).await;
  }

  /// What a run was launched with, each piece of what went into its prompt with how it went in where it says:
  /// held, where the session already holds that version, or attached in full.
  pub async fn launched_holding(
    &mut self,
    db: &DatabaseConnection,
    system: Option<&Path>,
    prompt: &str,
    context: &[(String, String, Option<&'static str>)],
  ) {
    if let Some(text) = system.and_then(|path| std::fs::read_to_string(path).ok()) {
      let version = format!("{:x}", sha2::Sha256::digest(text.as_bytes()));
      let latest = one(
        db,
        r#"SELECT "ref" FROM "agent_log" WHERE "agent" = $1 AND "kind" = 'prompt' AND "name" = $2 ORDER BY "id" DESC LIMIT 1"#,
        vec![self.target.agent.clone().into(), SYSTEM_PROMPT_LINE.into()],
      )
      .await
      .ok()
      .flatten()
      .and_then(|row| row.try_get::<Option<String>>("", "ref").ok().flatten());

      if latest.as_deref() != Some(version.as_str()) {
        let line = Line {
          kind: "prompt",
          name: Some(SYSTEM_PROMPT_LINE.to_owned()),
          reference: Some(version),
          text,
          ..Line::default()
        };

        self.write(db, line).await;
      }
    }

    self
      .write(
        db,
        Line {
          kind: "prompt",
          text: prompt.to_owned(),
          ..Line::default()
        },
      )
      .await;

    for (source, text, state) in context {
      let line = Line {
        kind: "context",
        name: Some(source.clone()),
        text: text.clone(),
        state: *state,
        ..Line::default()
      };

      self.write(db, line).await;
    }
  }

  /// What a run printed on its stream as it printed it, each line read as the event it is and filed under the
  /// road it went out on, every event counted; the lines written, for whoever watches the run go.
  pub async fn feed_stream(
    &mut self,
    db: &DatabaseConnection,
    printed: &[String],
    source: &str,
  ) -> Vec<Line> {
    let mut written = vec![];

    for printed in printed {
      let trimmed = printed.trim();

      if !trimmed.starts_with('{') {
        continue;
      }

      // A line that is not JSON is no event, and neither is the broken tail of a run that was killed.
      let Some(event) = Json::parse(trimmed) else { continue };

      self.source_event += 1;
      self.tally.see(&event);

      for line in stream_lines(&event) {
        let line = Line {
          source: Some(source.to_owned()),
          source_event: Some(self.source_event),
          ..line
        };

        self.write(db, line.clone()).await;
        written.push(line);
      }
    }

    written
  }

  /// What a run of `codex exec` printed, each line read as the event it is and filed under the road it went
  /// out on; where the log is a job's, each change's diffs are read off the thread's record under `sessions`,
  /// at once where it holds them and filled in once it does.
  pub async fn feed_codex(
    &mut self,
    db: &DatabaseConnection,
    records: &[String],
    source: &str,
    sessions: &Path,
  ) {
    for record in records {
      let trimmed = record.trim();

      if !trimmed.starts_with('{') {
        continue;
      }

      let Some(event) = Json::parse(trimmed) else { continue };

      self.source_event += 1;
      self.codex_started(&event, sessions);

      for line in codex_lines(&event) {
        let line = Line {
          source: Some(source.to_owned()),
          source_event: Some(self.source_event),
          ..line
        };

        self.write_codex(db, line).await;
      }

      self.settle_codex(db).await;
    }
  }

  /// The diffs of the changes written before the record held them, filled in where it holds them by now.
  pub async fn settle_codex(&mut self, db: &DatabaseConnection) {
    let Some(changes) = self.codex.as_mut() else { return };

    for (held, diffs) in changes.settle() {
      let metadata = with_diffs(held.metadata, diffs).stringify();
      let filled = run(
        db,
        r#"UPDATE "agent_log" SET "metadata" = $1, "metadata_length" = $2 WHERE "id" = $3"#,
        vec![metadata.clone().into(), (metadata.chars().count() as i64).into(), held.id.into()],
      )
      .await;

      if let Err(why) = filled {
        eprintln!("{}: a line's metadata: {why}", self.target.agent);
      }
    }
  }

  // The thread a job's Codex run is on, whose record its changes are read off.
  fn codex_started(&mut self, event: &Json, sessions: &Path) {
    if self.target.job_id.is_none() || event.get("type").and_then(Json::as_str) != Some("thread.started") {
      return;
    }

    let Some(thread) = event.get("thread_id").and_then(Json::as_str) else { return };

    self.codex.get_or_insert_with(|| CodexChanges::new(sessions.to_owned())).started(thread);
  }

  // A line of a Codex run, a change's result carrying the diffs the record holds of it.
  async fn write_codex(&mut self, db: &DatabaseConnection, line: Line) {
    let item = line.metadata.as_ref().and_then(|metadata| metadata.get("item"));
    let paths = item
      .filter(|item| {
        line.kind == "tool-result" && item.get("type").and_then(Json::as_str) == Some("file_change")
      })
      .and_then(stream_paths);
    let (Some(paths), Some(changes)) = (paths, self.codex.as_mut()) else {
      self.write(db, line).await;
      return;
    };
    let metadata = line.metadata.clone().unwrap_or(Json::Object(vec![]));

    match changes.take(&paths) {
      Some(diffs) => {
        self
          .write(
            db,
            Line {
              metadata: Some(with_diffs(metadata, diffs)),
              ..line
            },
          )
          .await;
      }
      None => {
        if let Some(id) = self.write(db, line).await
          && let Some(changes) = self.codex.as_mut()
        {
          changes.wait(Waiting {
            paths,
            id,
            metadata,
          });
        }
      }
    }
  }

  /// Where the run's time went, as far as its lines have got.
  pub fn spans(&self) -> Vec<JobSpan> {
    self.tally.spans()
  }
}

// A change's metadata with the diffs of the files it changed beside it.
fn with_diffs(metadata: Json, diffs: Vec<Json>) -> Json {
  let mut fields = match metadata {
    Json::Object(fields) => fields,
    _ => vec![],
  };

  match fields.iter_mut().find(|(name, _)| name == "fileDiffs") {
    Some((_, held)) => *held = Json::Array(diffs),
    None => fields.push(("fileDiffs".to_owned(), Json::Array(diffs))),
  }

  Json::Object(fields)
}

// One agent's own lines held to the run log's bounds.
async fn prune(db: &DatabaseConnection, agent: &str) -> Result<(), Refusal> {
  prune::agent_run_log(db, agent, now_ms()).await.map(|_| ())
}

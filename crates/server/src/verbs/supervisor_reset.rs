//! `bmo-agency supervisor reset <agent>`: a supervisor started afresh in the ranges the caller names
//! (docs/supervisors.md, 「Starting it afresh」). The reset is the server's route, so this verb only asks it
//! and prints what it answered: a dry run counts what would go and writes nothing.

use clap::Parser;
use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use serde_json::{Value, json};

use super::{Answer, CONTROL_TOKEN, VerbError, control_call, control_port, control_token};

/// What `bmo-agency supervisor reset` is asked.
#[derive(Parser, Debug, Default)]
pub struct ResetArgs {
  /// The supervisor's agent name.
  pub agent: String,
  /// Take what it said in its rooms; both ranges where neither is named.
  #[arg(long)]
  pub conversation: bool,
  /// Take what it remembers; both ranges where neither is named.
  #[arg(long)]
  pub person: bool,
  /// Count what would go and write nothing.
  #[arg(long = "dry-run")]
  pub dry_run: bool,
  /// A room it answers alone, repeatable or comma separated.
  #[arg(long, value_delimiter = ',', allow_negative_numbers = true)]
  pub alone: Vec<i64>,
  /// A room it shares with other agents, repeatable or comma separated.
  #[arg(long, value_delimiter = ',', allow_negative_numbers = true)]
  pub shared: Vec<i64>,
}

fn text(value: &Value) -> String {
  match value {
    Value::String(text) => text.clone(),
    Value::Null => String::new(),
    other => other.to_string(),
  }
}

fn rooms(value: Option<&Value>) -> String {
  let words = &VERB_WORDS.supervisor_reset;
  let listed: Vec<String> = value
    .and_then(Value::as_array)
    .into_iter()
    .flatten()
    .map(|room| {
      let id = room.get("chatId").and_then(Value::as_i64).unwrap_or_default().to_string();

      match room
        .get("title")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|title| !title.is_empty())
      {
        Some(title) => fill_marks(&words.room, &[("title", title), ("room", &id)]),
        None => fill_marks(&words.room_untitled, &[("room", &id)]),
      }
    })
    .collect();

  if listed.is_empty() { words.none.clone() } else { listed.join(", ") }
}

// What the brain would take, or took: its counts added up, and what it could not reach.
fn brain(value: &Value) -> Option<String> {
  let words = &VERB_WORDS.supervisor_reset;

  if value.is_null() {
    return None;
  }

  let took = value
    .get("took")
    .and_then(Value::as_object)
    .map(|counts| counts.values().filter_map(Value::as_i64).sum::<i64>())
    .unwrap_or_default();
  let could_not = value.get("couldNot").and_then(Value::as_array).map(Vec::len).unwrap_or_default();

  Some(fill_marks(
    &words.brain,
    &[("took", &took.to_string()), ("could_not", &could_not.to_string())],
  ))
}

/// The lines a reset prints, nothing beside what the route said.
pub fn reset_lines(answer: &Value, dry_run: bool) -> Vec<String> {
  let words = &VERB_WORDS.supervisor_reset;
  let ranges: Vec<&str> =
    answer["ranges"].as_array().into_iter().flatten().filter_map(Value::as_str).collect();
  let ranges_text: Vec<&str> = ranges
    .iter()
    .map(|range| {
      if *range == "conversation" {
        words.conversation.as_str()
      } else {
        words.person.as_str()
      }
    })
    .collect();
  let rows = answer["rows"].as_array().cloned().unwrap_or_default();
  let total: i64 = rows.iter().filter_map(|row| row["rows"].as_i64()).sum();
  let files: Vec<String> = answer["files"].as_array().into_iter().flatten().map(text).collect();
  let mut lines = vec![fill_marks(
    &words.head,
    &[
      ("label", &text(&answer["label"])),
      ("agent", &text(&answer["agent"])),
      ("mode", if dry_run { &words.preview } else { &words.run }),
      ("ranges", &ranges_text.join(", ")),
    ],
  )];

  if ranges.contains(&"conversation") {
    lines.push(fill_marks(&words.alone, &[("rooms", &rooms(answer.get("alone")))]));
    lines.push(fill_marks(&words.shared, &[("rooms", &rooms(answer.get("shared")))]));
  }

  lines.push(fill_marks(&words.rows, &[("count", &total.to_string())]));

  for row in rows.iter().filter(|row| row["rows"].as_i64().unwrap_or_default() > 0) {
    lines.push(fill_marks(
      &words.row,
      &[
        ("table", &text(&row["table"])),
        ("range", &text(&row["range"])),
        ("rows", &text(&row["rows"])),
      ],
    ));
  }

  lines.push(fill_marks(&words.files, &[("count", &files.len().to_string())]));
  lines.extend(files.iter().map(|path| fill_marks(&words.file, &[("path", path)])));

  if let Some(brain) = brain(&answer["brain"]) {
    lines.push(brain);
  }

  for failure in answer["failures"].as_array().into_iter().flatten() {
    lines.push(fill_marks(&words.failure, &[("failure", &text(failure))]));
  }

  lines
}

/// A supervisor started afresh, by the server's route on the control port; a dry run counts what would go
/// and changes nothing.
pub async fn reset(args: ResetArgs) -> Result<Answer, VerbError> {
  let words = &VERB_WORDS.supervisor_reset;
  let Some(token) = control_token() else {
    return Err(VerbError::Refused(fill_marks(&words.no_token, &[("variable", CONTROL_TOKEN)])));
  };
  let named = args.conversation || args.person;
  let body = json!({
    "conversation": !named || args.conversation,
    "person": !named || args.person,
    "alone": args.alone,
    "shared": args.shared,
    "dryRun": args.dry_run,
    "by": "shell",
  });
  // The reset is waited on however long it takes: it may stop a writer's run and walk every record the
  // supervisor kept.
  let path = format!("/supervisors/{}/resets", args.agent);
  let (status, answer) =
    control_call(&token, reqwest::Method::POST, &path, Some(&body)).await.map_err(|why| {
      VerbError::Refused(fill_marks(
        &words.unanswered,
        &[("port", &control_port().to_string()), ("why", &why.to_string())],
      ))
    })?;
  let answer = answer.unwrap_or(Value::Null);

  if !(200..300).contains(&status) || answer.get("agent").is_none() {
    let why = answer
      .get("error")
      .and_then(Value::as_str)
      .map_or_else(|| format!("the server answered {status}"), str::to_owned);

    return Err(VerbError::Refused(fill_marks(&words.refused, &[("why", &why)])));
  }

  Ok(Answer::from(reset_lines(&answer, args.dry_run)))
}

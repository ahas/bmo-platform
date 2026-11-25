//! A question a Claude Code program puts to a person in the middle of a turn, read off the screen, the
//! keys that answer it, and what one turn does with the questions its program puts (docs/agent-control.md,
//! 「A confirmation in the middle of a turn」).
//!
//! The program keeps a few safety checks that ask a person even in bypass mode. The screen draws such a
//! question below the last rule, as a few lines of what is asked and a numbered choice whose cursor row is
//! marked; a `Yes` and a `No` row make it a permission question. Anything else that holds the pane blocked
//! is a question this reading does not know, and what is kept of it is the screen it stood at.

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;
use std::time::Duration;

use herdr::Herdr;
use policy::door_words::DOOR_WORDS;
use policy::fill_marks;
use regex::Regex;
use serde_json::json;
use tokio::sync::oneshot;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use super::looks::{Look, screen_of, send_keys};
use super::turn::Watch;

/// The mark a job question carries when it is a pane's confirmation put by the runner rather than
/// something the job asked itself.
pub fn confirmation_mark() -> &'static str {
  &DOOR_WORDS.confirm.mark
}

/// A question on the screen: one this reading knows, bounded by the rule above and the choices below it,
/// or one it does not, known by the foot of the screen, which is evidence and never words to hand on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirmation {
  Permission { text: String, amendable: bool },
  Unknown { screen: String },
}

impl Confirmation {
  /// What tells one standing question from the next.
  pub fn key(&self) -> &str {
    match self {
      Self::Permission { text, .. } => text,
      Self::Unknown { screen } => screen,
    }
  }

  fn amendable(&self) -> bool {
    matches!(
      self,
      Self::Permission {
        amendable: true,
        ..
      }
    )
  }
}

/// What the caller decided: allow the command, refuse it with the words the run is given in its place,
/// or leave the question for a person to answer in herdr.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ConfirmAnswer {
  Allow,
  Refuse(String),
  Leave,
}

/// A question put to a turn's caller: where its answer, or why it could not be asked, arrives, and how it
/// is withdrawn once the pane stops asking.
pub struct Asking {
  pub answer: oneshot::Receiver<Result<ConfirmAnswer, String>>,
  pub withdraw: CancellationToken,
}

const CURSOR_MARK: &str = "❯";
// The longest an unknown question is kept at: the bottom of the screen is where a question stands.
const UNKNOWN_ROWS: usize = 15;
// How many times one answer is typed into a question that stays on the screen after it, before the
// question is left for a person.
const ANSWER_TRIES: u32 = 3;

static OPTION_ROW: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"^\s*(?:❯\s*)?(\d+)\.\s+(.*?)\s*$").expect("an option row pattern"));
static HINT_ROW: LazyLock<Regex> =
  LazyLock::new(|| Regex::new(r"Esc to cancel|Enter to confirm").expect("a hint row pattern"));
// The line a standing question counts down on, redrawn every second: kept, every look would be a new one.
static COUNTDOWN_ROW: LazyLock<Regex> = LazyLock::new(|| {
  Regex::new(r"\bwill automatically \w+ this request in \d+:\d{2}\b").expect("a countdown pattern")
});
static RULE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*─{10,}\s*$").expect("a rule pattern"));
static BLANK_RUNS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\n{3,}").expect("a blank run pattern"));

fn screen_rows(screen: &str) -> Vec<&str> {
  screen.split('\n').filter(|row| !COUNTDOWN_ROW.is_match(row)).collect()
}

fn dialog_rows(screen: &str) -> Vec<&str> {
  let rows = screen_rows(screen);
  let rule = rows.iter().rposition(|row| RULE.is_match(row));

  rows[rule.map_or(0, |at| at + 1)..].to_vec()
}

struct Choice {
  at: usize,
  text: String,
  cursor: bool,
}

fn choices(rows: &[&str]) -> Vec<Choice> {
  rows
    .iter()
    .enumerate()
    .filter_map(|(at, row)| {
      OPTION_ROW.captures(row).map(|found| Choice {
        at,
        text: found[2].to_owned(),
        cursor: row.trim_start().starts_with(CURSOR_MARK),
      })
    })
    .collect()
}

fn tidy(rows: &[&str]) -> String {
  let joined = rows.iter().map(|row| row.trim_end()).collect::<Vec<_>>().join("\n");

  BLANK_RUNS.replace_all(&joined, "\n\n").trim().to_owned()
}

fn starts_with_word(text: &str, word: &str) -> bool {
  text
    .strip_prefix(word)
    .is_some_and(|rest| rest.chars().next().is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_')))
}

/// The question on the screen, or none where the screen holds no readable question yet.
pub fn confirmation_of(screen: &str) -> Option<Confirmation> {
  let rows = dialog_rows(screen);
  let options = choices(&rows);
  let amendable = rows.iter().any(|row| row.contains("Tab to amend"));
  let permission = options.iter().any(|one| starts_with_word(&one.text, "Yes"))
    && options.iter().any(|one| starts_with_word(&one.text, "No"));

  if permission {
    let first = options.first().map_or(rows.len(), |one| one.at);
    let asked: Vec<&str> = rows[..first].iter().copied().filter(|row| !HINT_ROW.is_match(row)).collect();
    let text = tidy(&asked);

    return (!text.is_empty()).then_some(Confirmation::Permission { text, amendable });
  }

  let kept: Vec<&str> = screen_rows(screen)
    .into_iter()
    .filter(|row| !row.trim().is_empty() && !RULE.is_match(row))
    .collect();
  let foot = tidy(&kept[kept.len().saturating_sub(UNKNOWN_ROWS)..]);

  (!foot.is_empty()).then_some(Confirmation::Unknown { screen: foot })
}

/// The keys that move the cursor onto the row whose choice starts with the given word, or none where the
/// screen has no such row or no cursor.
pub fn choice_keys(screen: &str, choice: &str) -> Option<Vec<&'static str>> {
  let options = choices(&dialog_rows(screen));
  let target = options.iter().position(|one| starts_with_word(&one.text, choice))?;
  let cursor = options.iter().position(|one| one.cursor)?;
  let steps = target as i64 - cursor as i64;

  Some(vec![if steps > 0 { "down" } else { "up" }; steps.unsigned_abs() as usize])
}

/// The words a run is given where a command of its stopped on a confirmation and was refused.
pub fn refusal_words(asked: &Confirmation, owner: Option<&str>) -> String {
  let mut words = vec![
    owner.map_or_else(
      || "Refused. This stopped on a Claude Code confirmation, and a job's pane has nobody to answer one, so it was not run.".to_owned(),
      |said| format!("Refused by {}, who said: {said}", policy::deployment::DEPLOYMENT.owner_name),
    ),
    "Do not retry it as written.".to_owned(),
    r#"For a removal through a variable, name the path outright or guard and quote the variable — rm -f "${S:?}"/*.md — and never remove through a $(…); leave files under ~/.claude alone."#.to_owned(),
    "Then carry on with the work in this same turn.".to_owned(),
  ];

  if matches!(asked, Confirmation::Unknown { .. }) {
    words.push(
      "The pane stopped at a question this reading does not know, and that question was closed. What stood on the screen is in the run's log."
        .to_owned(),
    );
  }

  words.join(" ")
}

// A question put to the caller, and its answer once it has come.
struct Pending {
  asked: Confirmation,
  withdraw: CancellationToken,
  answer: Option<oneshot::Receiver<Result<ConfirmAnswer, String>>>,
  got: Option<ConfirmAnswer>,
}

// The answer last typed in and not yet seen gone.
struct Typed {
  key: String,
  at: Instant,
  tries: u32,
}

/// The questions one turn's pane puts to its caller while the turn is watched: the one out now, the ones
/// left for a person, the answer each question already had, the one last typed in, and the words a
/// refusal still owes the run where the screen could not carry them.
pub struct Questions {
  asking: Option<Pending>,
  left: HashSet<String>,
  decided: HashMap<String, ConfirmAnswer>,
  typed: Option<Typed>,
  words: Option<String>,
  redraw: Duration,
}

impl Questions {
  pub fn new(redraw: Duration) -> Self {
    Self {
      asking: None,
      left: HashSet::new(),
      decided: HashMap::new(),
      typed: None,
      words: None,
      redraw,
    }
  }

  /// The words a refusal owes the run, until they have gone in.
  pub fn owed(&self) -> Option<String> {
    self.words.clone()
  }

  pub fn paid(&mut self) {
    self.words = None;
  }

  /// A question still being put is about a pane that is going away.
  pub fn withdraw(&mut self) {
    if let Some(asking) = &self.asking {
      asking.withdraw.cancel();
    }
  }

  // An answer that came while the turn was watched, taken in.
  async fn collect<W: Watch>(&mut self, watch: &mut W) {
    let Some(asking) = self.asking.as_mut() else { return };
    let Some(receiver) = asking.answer.as_mut() else { return };

    match receiver.try_recv() {
      Ok(Ok(answer)) => {
        asking.got = Some(answer);
        asking.answer = None;
      }
      Ok(Err(why)) => {
        asking.got = Some(ConfirmAnswer::Leave);
        asking.answer = None;
        watch.note(&fill_marks(&DOOR_WORDS.confirm.unasked, &[("why", &why.to_string())])).await;
      }
      Err(oneshot::error::TryRecvError::Empty) => {}
      Err(oneshot::error::TryRecvError::Closed) => {
        asking.got = Some(ConfirmAnswer::Leave);
        asking.answer = None;
        watch
          .note(&fill_marks(&DOOR_WORDS.confirm.unasked, &[("why", "the asking ended with no answer")]))
          .await;
      }
    }
  }

  /// One look at the pane, in the state herdr reads it in.
  pub async fn look<W: Watch>(&mut self, herdr: &Herdr, pane: &str, now: &Look, watch: &mut W) {
    self.collect(watch).await;

    if now.is("blocked") && watch.puts_questions() {
      let screen = screen_of(herdr, pane).await;
      let asked = confirmation_of(&screen);
      let key = asked.as_ref().map_or_else(String::new, |asked| asked.key().to_owned());

      if let Some(answer) = self.asking.as_ref().and_then(|asking| asking.got.clone()) {
        let pending = self.asking.take().expect("an answered question is pending");
        let asked_key = pending.asked.key().to_owned();

        self.decided.insert(asked_key.clone(), answer.clone());

        match &asked {
          Some(asked) if key == asked_key => {
            self.answer(herdr, pane, &screen, asked, &answer, 1, watch).await
          }
          _ => watch.note(&DOOR_WORDS.confirm.changed).await,
        }

        return;
      }

      if !self.settle_typed(herdr, pane, &screen, asked.as_ref(), &key, watch).await
        || self.asking.is_some()
      {
        return;
      }

      let Some(asked) = asked else { return };

      if self.left.contains(&key) {
        return;
      }

      if let Some(given) = self.decided.get(&key).cloned() {
        watch.note(&DOOR_WORDS.confirm.repeated).await;
        self.answer(herdr, pane, &screen, &asked, &given, 1, watch).await;
        return;
      }

      self.put(&screen, asked, watch).await;
      return;
    }

    if self.typed.is_some() {
      self.typed_gone(watch).await;
    }

    if let Some(asking) = self.asking.take() {
      // The pane stopped asking before the answer came: a person answered it where it stands.
      asking.withdraw.cancel();
      watch.note(&DOOR_WORDS.confirm.closed_early).await;
    }
  }

  // Typing an answer into the question on the screen, and remembering it was typed so the next looks can
  // see whether the question went.
  #[allow(clippy::too_many_arguments)]
  async fn answer<W: Watch>(
    &mut self,
    herdr: &Herdr,
    pane: &str,
    screen: &str,
    asked: &Confirmation,
    answer: &ConfirmAnswer,
    tries: u32,
    watch: &mut W,
  ) {
    let key = asked.key().to_owned();

    if let Some(words) = self.answer_confirmation(herdr, pane, screen, asked, answer, watch).await {
      self.words = Some(words);
    }

    self.typed = (!self.left.contains(&key)).then(|| Typed {
      key,
      at: Instant::now(),
      tries,
    });
  }

  // Answering a question in the pane the way the caller decided, and the words still owed to the run
  // where the screen could not carry them. The log is told either way.
  async fn answer_confirmation<W: Watch>(
    &mut self,
    herdr: &Herdr,
    pane: &str,
    screen: &str,
    asked: &Confirmation,
    answer: &ConfirmAnswer,
    watch: &mut W,
  ) -> Option<String> {
    let permission = matches!(asked, Confirmation::Permission { .. });

    match answer {
      ConfirmAnswer::Leave => {
        self.left.insert(asked.key().to_owned());
        watch.note(&DOOR_WORDS.confirm.left_to_person).await;
        None
      }
      ConfirmAnswer::Allow => {
        let Some(mut moves) = permission.then(|| choice_keys(screen, "Yes")).flatten() else {
          self.left.insert(asked.key().to_owned());
          watch.note(&DOOR_WORDS.confirm.no_allow).await;
          return None;
        };

        moves.push("enter");
        let _ = send_keys(herdr, pane, &moves).await;
        watch.note(&DOOR_WORDS.confirm.allowed).await;
        None
      }
      ConfirmAnswer::Refuse(reason) => {
        let moves = permission.then(|| choice_keys(screen, "No")).flatten();

        if let Some(moves) = &moves
          && asked.amendable()
        {
          // The refusal row takes words of its own once `tab` opens it, and those words reach the run in
          // the refused command's place, inside the same turn. A line break would send them half typed.
          let mut opening = moves.clone();

          opening.push("tab");
          let _ = send_keys(herdr, pane, &opening).await;
          let _ = herdr
            .call("pane.send_text", json!({ "pane_id": pane, "text": single_line(reason) }), None)
            .await;
          let _ = send_keys(herdr, pane, &["enter"]).await;
          watch.note(&DOOR_WORDS.confirm.refused_with_reason).await;
          return None;
        }

        let keys = match &moves {
          None => vec!["esc"],
          Some(moves) => moves.iter().copied().chain(["enter"]).collect(),
        };

        let _ = send_keys(herdr, pane, &keys).await;
        watch
          .note(if moves.is_none() { &DOOR_WORDS.confirm.closed } else { &DOOR_WORDS.confirm.refused })
          .await;
        Some(reason.clone())
      }
    }
  }

  // Whether the answer last typed has been seen through: the question it answered gone from the screen,
  // or stood on past every try and left for a person. Otherwise the look is spent on it.
  async fn settle_typed<W: Watch>(
    &mut self,
    herdr: &Herdr,
    pane: &str,
    screen: &str,
    asked: Option<&Confirmation>,
    key: &str,
    watch: &mut W,
  ) -> bool {
    let Some(typed) = &self.typed else { return true };

    // A screen not readable yet says nothing either way.
    let Some(asked) = asked else { return false };

    if typed.key != key {
      self.typed_gone(watch).await;
      return true;
    }

    // A question just answered can still be on the screen for a look or two.
    if typed.at.elapsed() < self.redraw {
      return false;
    }

    let tries = typed.tries;

    if tries >= ANSWER_TRIES {
      self.left.insert(key.to_owned());
      self.typed = None;
      watch
        .note(&fill_marks(&DOOR_WORDS.confirm.tries, &[("tries", &ANSWER_TRIES.to_string())]))
        .await;
      return false;
    }

    let Some(answer) = self.decided.get(key).cloned() else {
      self.typed = None;
      return true;
    };

    watch.note(&DOOR_WORDS.confirm.again).await;
    self.answer(herdr, pane, screen, asked, &answer, tries + 1, watch).await;
    false
  }

  async fn typed_gone<W: Watch>(&mut self, watch: &mut W) {
    watch.note(&DOOR_WORDS.confirm.answered).await;
    self.typed = None;
  }

  // Putting a question to the caller, which decides while the looks go on.
  async fn put<W: Watch>(&mut self, screen: &str, asked: Confirmation, watch: &mut W) {
    // A question this reading knows is named by its own first line; one it does not know has no question
    // to name, so the whole of what stood there is kept here and nowhere else.
    let named = match &asked {
      Confirmation::Permission { text, .. } => fill_marks(
        &DOOR_WORDS.confirm.stopped_at,
        &[(
          "question",
          text.split('\n').find(|row| !row.trim().is_empty()).map(str::trim).unwrap_or_default(),
        )],
      ),
      Confirmation::Unknown { .. } => fill_marks(&DOOR_WORDS.screens.unknown_window, &[("screen", screen)]),
    };

    watch.note(&named).await;

    let pending = match watch.confirm(&asked).await {
      Some(asking) => Pending {
        asked,
        withdraw: asking.withdraw,
        answer: Some(asking.answer),
        got: None,
      },
      None => Pending {
        asked,
        withdraw: CancellationToken::new(),
        answer: None,
        got: Some(ConfirmAnswer::Leave),
      },
    };

    self.asking = Some(pending);
  }
}

// Words typed into one row of the screen: every line break and the space around it made one space.
fn single_line(text: &str) -> String {
  static BREAKS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\s*\n\s*").expect("a line break pattern"));

  BREAKS.replace_all(text, " ").into_owned()
}

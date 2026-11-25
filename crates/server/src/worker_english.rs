//! What a person wrote for a job's worker, put into the worker's language on its way in (docs/worker-language.md,
//! 「What agency translates on the way in」): the owner's part of a job's preamble, the words handed to a running
//! job and an item's title. The gate reads each text and keeps what it translated; a hand-over waits for its answer
//! no longer than the policy's bound, and a text the gate did not render whole in time goes in as written under the
//! mark, a line over a text and words after a title.

use std::sync::Arc;
use std::time::Duration;

use bmo_gate_client::Reading;
use futures_util::future::join_all;
use policy::job_prompt::JOB_PROMPT;

use crate::peers::gate::Gate;

/// Each text in the worker's language, in the order given: a text with no letter outside the Latin script as it
/// stands, without asking the gate; a reading the gate answered whole as it answered it; and any other text, a
/// reading the gate answered only in part, or none in time, under the untranslated mark.
pub async fn english(gate: &dyn Gate, texts: Vec<String>) -> Vec<String> {
  rendered(gate, texts, marked).await
}

/// Each title in the worker's language, as `english` reads a text, but for a title the gate did not render whole in
/// time standing as written with the untranslated words after it, since a title stands inside a line.
pub async fn english_titles(gate: &dyn Gate, titles: Vec<String>) -> Vec<String> {
  rendered(gate, titles, marked_title).await
}

/// A title asked of the gate ahead of any job, so its translation is kept by the time a worker reads it: asked in a
/// task of its own that nothing waits on, bounded as every hand-over is, and nothing asked for a title in the Latin
/// script.
pub fn ask_ahead(gate: Arc<dyn Gate>, title: String) {
  if !foreign(&title) {
    return;
  }

  tokio::spawn(async move {
    read(gate.as_ref(), &[title]).await;
  });
}

// Each text in the worker's language, a text the gate did not render whole in time put under `mark`.
async fn rendered(gate: &dyn Gate, texts: Vec<String>, mark: fn(&str) -> String) -> Vec<String> {
  let asked: Vec<usize> = (0..texts.len()).filter(|at| foreign(&texts[*at])).collect();

  if asked.is_empty() {
    return texts;
  }

  let originals: Vec<String> = asked.iter().map(|at| texts[*at].clone()).collect();
  let readings = read(gate, &originals).await;
  let mut texts = texts;

  for (at, reading) in asked.into_iter().zip(readings) {
    texts[at] = match reading {
      Some(reading) if reading.whole => reading.text,
      Some(reading) => mark(&reading.text),
      None => mark(&texts[at]),
    };
  }

  texts
}

/// One text in the worker's language, as `english` reads it.
pub async fn english_one(gate: &dyn Gate, text: &str) -> String {
  english(gate, vec![text.to_owned()]).await.pop().unwrap_or_default()
}

// Each original's reading, in calls of no more originals than one call to the gate carries, all of them waited for
// together within the bound; none for an original whose call failed, answered out of shape or was not answered in
// time.
async fn read(gate: &dyn Gate, originals: &[String]) -> Vec<Option<Reading>> {
  let words = &JOB_PROMPT.translation;
  let chunks: Vec<&[String]> = originals.chunks(words.most_originals.max(1)).collect();
  let calls = chunks.iter().map(|chunk| gate.translate(&words.language, chunk.to_vec()));
  let Ok(answers) = tokio::time::timeout(Duration::from_millis(words.wait_ms), join_all(calls)).await
  else {
    eprintln!(
      "worker english: the gate did not answer within {} ms, so {} texts go in as written",
      words.wait_ms,
      originals.len()
    );

    return vec![None; originals.len()];
  };

  chunks
    .iter()
    .zip(answers)
    .flat_map(|(chunk, answer)| match answer {
      Ok(readings) if readings.len() == chunk.len() => readings.into_iter().map(Some).collect(),
      Ok(readings) => {
        eprintln!(
          "worker english: the gate answered {} readings for {} texts, so they go in as written",
          readings.len(),
          chunk.len()
        );

        vec![None; chunk.len()]
      }
      Err(fault) => {
        eprintln!("worker english: {fault}, so {} texts go in as written", chunk.len());

        vec![None; chunk.len()]
      }
    })
    .collect()
}

// A text under the line saying it could not be translated and stands in its original language.
fn marked(text: &str) -> String {
  format!("{}\n\n{text}", JOB_PROMPT.translation.untranslated)
}

// A title with the words saying it could not be translated after it.
fn marked_title(title: &str) -> String {
  format!("{title} {}", JOB_PROMPT.translation.untranslated_title)
}

/// Whether a text holds a letter outside the Latin script, and so is the gate's to read: a text with none is left
/// as written by the gate in every part, so it is never sent.
pub fn foreign(text: &str) -> bool {
  text.chars().any(|letter| letter.is_alphabetic() && !latin(letter))
}

// Whether a letter is one of the Latin script's: the basic letters, the Latin-1 and extended blocks, the phonetic
// and modifier letters written beside them, and the Latin ligatures.
fn latin(letter: char) -> bool {
  matches!(
    letter,
    'a'..='z'
      | 'A'..='Z'
      | '\u{AA}'
      | '\u{B5}'
      | '\u{BA}'
      | '\u{C0}'..='\u{2FF}'
      | '\u{1D00}'..='\u{1DBF}'
      | '\u{1E00}'..='\u{1EFF}'
      | '\u{2C60}'..='\u{2C7F}'
      | '\u{A720}'..='\u{A7FF}'
      | '\u{FB00}'..='\u{FB06}'
  )
}

//! What a person wrote for a job's worker, put into the worker's language on its way in (docs/worker-language.md,
//! 「What agency translates on the way in」, 「Where the translation is made」, 「When there is no translation」): a
//! Latin text goes as written and asks nothing, any other goes as the gate read it, and one the gate did not read
//! whole in time goes as written under the mark, a title with the mark after it.

use agency_server::peers::gate::NoGate;
use agency_server::worker_english::{english, english_one, english_titles};
use policy::job_prompt::JOB_PROMPT;

use crate::support::gates::{Answering, Translator};

const GREEK: &str = "Η κονσόλα βρίσκεται στη θύρα 3000.";
const ENGLISH: &str = "The console stands on port 3000.";

fn marked(text: &str) -> String {
  format!("{}\n\n{text}", JOB_PROMPT.translation.untranslated)
}

#[tokio::test]
async fn a_text_in_the_latin_script_goes_as_written_and_asks_nothing_of_the_gate() {
  let gate = Translator::default();
  let texts = vec![ENGLISH.to_owned(), "Le café est fermé à 18h.".to_owned()];

  assert_eq!(english(&gate, texts.clone()).await, texts);
  assert!(gate.calls().is_empty(), "{:?}", gate.calls());
}

#[tokio::test]
async fn a_text_in_another_script_goes_as_the_gate_read_it_into_the_worker_s_language() {
  let gate = Translator::reading(&[(GREEK, ENGLISH)]);

  assert_eq!(
    english(&gate, vec!["Check the port.".to_owned(), GREEK.to_owned()]).await,
    ["Check the port.", ENGLISH]
  );
  assert_eq!(
    gate.calls(),
    [(JOB_PROMPT.translation.language.clone(), vec![GREEK.to_owned()])],
    "only the text in another script is asked, in the worker's language"
  );
}

#[tokio::test]
async fn the_texts_of_one_hand_over_go_in_calls_the_gate_takes_and_come_back_in_their_order() {
  let most = JOB_PROMPT.translation.most_originals;
  let originals: Vec<String> = (0..=most).map(|at| format!("{GREEK} {at}")).collect();
  let pairs: Vec<(String, String)> =
    (0..=most).map(|at| (format!("{GREEK} {at}"), format!("{ENGLISH} {at}"))).collect();
  let gate = Translator::reading(
    &pairs
      .iter()
      .map(|(original, reading)| (original.as_str(), reading.as_str()))
      .collect::<Vec<_>>(),
  );
  let read = english(&gate, originals).await;

  assert_eq!(read, (0..=most).map(|at| format!("{ENGLISH} {at}")).collect::<Vec<_>>());
  assert_eq!(gate.calls().iter().map(|(_, asked)| asked.len()).collect::<Vec<_>>(), [most, 1]);
}

#[tokio::test]
async fn a_reading_the_gate_answered_in_part_goes_under_the_mark() {
  let gate = Translator::reading(&[(GREEK, ENGLISH)]).answering(Answering::InPart);

  assert_eq!(english_one(&gate, GREEK).await, marked(ENGLISH));
}

#[tokio::test]
async fn with_no_gate_a_text_in_another_script_goes_as_written_under_the_mark() {
  assert_eq!(english_one(&NoGate, GREEK).await, marked(GREEK));
  assert_eq!(english_one(&NoGate, ENGLISH).await, ENGLISH, "a Latin text is never marked");
}

// The bound is the policy's; the paused clock moves past it the moment nothing else is left to run.
#[tokio::test(start_paused = true)]
async fn a_gate_that_does_not_answer_within_the_bound_leaves_the_text_as_written_under_the_mark() {
  let gate = Translator::reading(&[(GREEK, ENGLISH)]).answering(Answering::Never);
  let started = tokio::time::Instant::now();

  assert_eq!(english_one(&gate, GREEK).await, marked(GREEK));
  assert!(
    started.elapsed().as_millis() >= u128::from(JOB_PROMPT.translation.wait_ms),
    "it waited the bound out"
  );
}

#[tokio::test]
async fn a_title_the_gate_did_not_read_whole_stands_with_the_mark_after_it() {
  let gate = Translator::reading(&[(GREEK, ENGLISH)]);
  let titled = |title: &str| format!("{title} {}", JOB_PROMPT.translation.untranslated_title);

  assert_eq!(english_titles(&gate, vec![GREEK.to_owned()]).await, [ENGLISH]);
  assert_eq!(
    english_titles(&NoGate, vec![GREEK.to_owned(), ENGLISH.to_owned()]).await,
    [titled(GREEK), ENGLISH.to_owned()]
  );
}

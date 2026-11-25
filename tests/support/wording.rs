//! What a turn's text carries: the data of what changed, and no sentence telling the model what to do.

// The sentences a turn used to carry, before they moved into the session's system prompt. A notice that
// brings one back has been written wrong even where its data still stands.
const INSTRUCTIONS: &[&str] = &[
  "Say in one line",
  "Correct ",
  "Read it with",
  "Read the item",
  "Write its",
  "put it to the owner",
  "never answer it",
  "launch it",
  "lease it",
  "run it",
  "Judge it",
  "approve it",
  "give lease",
  "Relay its",
  "Relay it",
  "never paste",
  "Do not ",
  "Nothing is asked",
  "file the malfunction",
  "reach any of them",
  "Nothing needs saying",
  "tell the owner",
  "why not",
  "job_lease_next",
  "assign_worker",
  "run_job",
  "job_item_update",
  "job_question",
  "job_replies",
];

/// Fails where `text` carries a sentence telling the model what to do rather than what changed.
pub fn assert_no_instruction(text: &str) {
  for phrase in INSTRUCTIONS {
    assert!(!text.contains(phrase), "{phrase:?} in {text}");
  }
}

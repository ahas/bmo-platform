//! What a game of the word chain says (`lang/<language>/word-chain.yaml`), each line with the `{name}`
//! marks its file names.

use serde::Deserialize;

/// What a game of the word chain says.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WordChainWords {
  pub lines: Lines,
  pub answers: Answers,
  pub book: Book,
}

/// The `book` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Book {
  pub installed: String,
  pub meanings: String,
  pub absent: String,
  pub found: String,
  pub loanword: String,
  pub basic: String,
  pub hard: String,
}

/// The `answers` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Answers {
  pub no_book: String,
  pub in_game: String,
  pub unusable: String,
  pub asked_again: String,
  pub opened: String,
  pub opened_more: String,
  pub none: String,
  pub several: String,
  pub not_theirs: String,
  pub unreached: String,
  pub not_taken: String,
}

/// The `lines` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lines {
  pub game: String,
  pub game_more: String,
  pub game_joint: String,
  pub opening_not_in_book: String,
  pub opening_loanword: String,
  pub opening_no_follower: String,
  pub ask_again: String,
  pub ask_again_bare: String,
  pub ask_first: String,
  pub no_opening: String,
  pub folded: String,
  pub call: String,
  pub call_join: String,
  pub call_join_named: String,
  pub call_how: String,
  pub call_opening: String,
  pub call_moves: String,
  pub rule_joint: String,
  pub rule_nouns: String,
  pub rule_loanwords_on: String,
  pub rule_loanwords_off: String,
  pub rule_manner_on: String,
  pub rule_manner_off: String,
  pub rule_no_limit: String,
  pub rule_limit: String,
  pub rule_robot: String,
  pub off_chain: String,
  pub not_in_book: String,
  pub loanword: String,
  pub used: String,
  pub manner: String,
  pub joined: String,
  pub started: String,
  pub order_joint: String,
  pub first_word: String,
  pub new_word: String,
  pub meaning: String,
  pub turn: String,
  pub turn_limit: String,
  pub taken: String,
  pub taken_robot: String,
  pub timeout: String,
  pub hanbang: String,
  pub hanbang_by: String,
  pub gave_up: String,
  pub stuck: String,
  pub nobody: String,
  pub stopped: String,
  pub idle: String,
  pub decided: String,
  pub ranking: String,
  pub pick: String,
  pub pick_stop: String,
  pub pick_join: String,
  pub pick_stop_word: String,
  pub pick_join_word: String,
  pub difficulty: LinesDifficulty,
}

/// The `difficulty` lines.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LinesDifficulty {
  pub easy: String,
  pub normal: String,
  pub hard: String,
}

crate::part!(pub WORD_CHAIN_WORDS: WordChainWords = "word-chain");

//! One account's token for the work nodes made and kept on this machine (docs/work-nodes.md, 「The logins on
//! a node」). The owner runs it once per account. Without `--paste` it runs `claude setup-token` under the
//! account's own directory, whose sign-in the owner approves in a browser signed in to that account; either
//! way the token is then pasted in, unechoed, or read from standard input where that is no terminal,
//! refused where it is not shaped as one, and written whole into the account's file, readable by this user
//! alone. Nothing of the token is printed, logged or kept anywhere else.

use std::io::{IsTerminal, Read, Write};
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

use policy::fill_marks;
use policy::verb_words::VERB_WORDS;
use sea_orm::DatabaseConnection;

use super::{Answer, VerbError};
use crate::control::claude_accounts::{Account, Credential, accounts};

/// Why a pasted text is not a token `claude setup-token` prints, or none: the CLI's long-lived tokens open
/// with `sk-ant-oat` and hold no space, and a short one is a paste cut off.
pub fn node_token_fault(token: &str) -> Option<String> {
  if !token.starts_with("sk-ant-oat") {
    return Some(VERB_WORDS.claude_token.not_a_token.clone());
  }

  (token.chars().any(char::is_whitespace) || token.encode_utf16().count() < 40)
    .then(|| VERB_WORDS.claude_token.cut.clone())
}

/// A token for the work nodes written whole into its file, readable by this user alone, its directory too:
/// a reader never sees half of one, and the file's moment is what tells this token from the last.
pub fn write_node_token(file: &Path, token: &str) -> std::io::Result<()> {
  if let Some(folder) = file.parent() {
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(folder)?;
  }

  let next = PathBuf::from(format!("{}.{}.next", file.display(), std::process::id()));
  let mut written = std::fs::OpenOptions::new()
    .write(true)
    .create(true)
    .truncate(true)
    .mode(0o600)
    .open(&next)?;

  written.write_all(format!("{token}\n").as_bytes())?;
  drop(written);
  std::fs::rename(next, file)
}

// The address the account's directory says it is signed in as, so the browser's sign-in is checked
// against it; none where the directory names none.
fn signed_in_as(account: &Account) -> Option<String> {
  let Credential::Directory { home } = &account.credential else { return None };
  let held = std::fs::read_to_string(home.join(".claude.json")).ok()?;
  let held: serde_json::Value = serde_json::from_str(&held).ok()?;

  held
    .pointer("/oauthAccount/emailAddress")
    .and_then(serde_json::Value::as_str)
    .map(str::to_owned)
}

// The terminal's echo turned off while a line is typed, and back on however the reading ends.
struct Unechoed(Option<libc::termios>);

impl Unechoed {
  fn on_standing_input() -> Self {
    // SAFETY: the structure is filled in by tcgetattr before it is read, and only on success.
    unsafe {
      let mut held: libc::termios = std::mem::zeroed();

      if libc::tcgetattr(libc::STDIN_FILENO, &mut held) != 0 {
        return Self(None);
      }

      let mut quiet = held;

      quiet.c_lflag &= !libc::ECHO;
      libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &quiet);

      Self(Some(held))
    }
  }
}

impl Drop for Unechoed {
  fn drop(&mut self) {
    if let Some(held) = self.0 {
      // SAFETY: the settings read before are handed back as they were.
      unsafe {
        libc::tcsetattr(libc::STDIN_FILENO, libc::TCSANOW, &held);
      }
    }
  }
}

// A line typed or pasted at the terminal without being shown, or standing input read whole where that is no
// terminal; none where the typing was given up. A paste the terminal brackets keeps none of its brackets.
fn hidden_line(prompt: &str) -> Option<String> {
  let mut input = std::io::stdin();

  if !input.is_terminal() {
    let mut whole = String::new();

    input.read_to_string(&mut whole).ok()?;
    return Some(whole.trim().to_owned());
  }

  print!("{prompt}");
  let _ = std::io::stdout().flush();

  let typed = {
    let _quiet = Unechoed::on_standing_input();
    let mut line = String::new();

    match input.read_line(&mut line) {
      Ok(0) | Err(_) => None,
      Ok(_) => Some(line),
    }
  };

  println!();
  typed.map(|line| line.replace("\u{1b}[200~", "").replace("\u{1b}[201~", "").trim().to_owned())
}

/// One account's token for the work nodes made, or kept as pasted, and written into its file.
pub async fn claude_token(
  db: &DatabaseConnection,
  data_root: &Path,
  named: &str,
  paste: bool,
) -> Result<Answer, VerbError> {
  let words = &VERB_WORDS.claude_token;
  let held = accounts(db).await.map_err(|refused| VerbError::Refused(refused.error))?;
  let account = held
    .iter()
    .find(|one| one.id == named)
    .or_else(|| held.iter().find(|one| one.name == named.trim()))
    .ok_or_else(|| VerbError::Refused(fill_marks(&words.unknown_account, &[("named", named)])))?;
  let Some(file) = account.node_token_file(data_root) else {
    return Err(VerbError::Refused(fill_marks(&words.endpoint_account, &[("name", &account.name)])));
  };

  println!("{}", fill_marks(&words.making, &[("name", &account.name)]));

  if !paste {
    println!(
      "{}",
      match signed_in_as(account) {
        Some(email) => fill_marks(&words.approve_as, &[("email", &email)]),
        None => words.approve.clone(),
      }
    );

    let mut sign_in = std::process::Command::new("claude");

    sign_in.arg("setup-token").env_remove("CLAUDE_CODE_OAUTH_TOKEN");

    if let Credential::Directory { home } = &account.credential {
      sign_in.env("CLAUDE_CONFIG_DIR", home);
    }

    if !sign_in.status().is_ok_and(|status| status.success()) {
      return Err(VerbError::Refused(words.sign_in_failed.clone()));
    }
  }

  let token = hidden_line(&words.paste).ok_or_else(|| VerbError::Refused(words.stopped.clone()))?;

  if let Some(fault) = node_token_fault(&token) {
    return Err(VerbError::Refused(fill_marks(&words.refused, &[("fault", &fault)])));
  }

  write_node_token(&file, &token).map_err(|why| VerbError::Refused(format!("claude-token: {why}")))?;

  Ok(Answer::from(vec![
    fill_marks(&words.kept, &[("file", &file.to_string_lossy())]),
    words.from_next_run.clone(),
  ]))
}

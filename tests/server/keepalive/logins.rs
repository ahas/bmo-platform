//! The logins' watch: a moment written as the TypeScript side writes a number, and the notices that name
//! the account, how long is left and the one command the owner runs.

use std::path::Path;

use agency_server::control::claude_accounts::{Account, Credential};
use agency_server::keepalive::logins::{NodeToken, moment_text, node_notice, node_token_lead, notice};
use policy::fill_marks;
use policy::providers::claude_code::CLAUDE_CODE;

fn account(kind: &str) -> Account {
  let path = "/logins/spare".into();

  Account {
    id: "spare".into(),
    name: "spare login".into(),
    place: 1,
    enabled: true,
    credential: match kind {
      "token" => Credential::Token { file: path },
      _ => Credential::Directory { home: path },
    },
    needs_login_at: None,
    owner_account: None,
    owner_unit: None,
    node: None,
    lendable: false,
    base_prompt: None,
    node_token_gone: None,
  }
}

#[test]
fn a_moment_is_written_as_the_typescript_side_writes_a_number() {
  assert_eq!(moment_text(1_712_345_678_901.0), "1712345678901");
  assert_eq!(moment_text(1_712_345_678_901.125), "1712345678901.125");
}

#[test]
fn the_warning_names_the_account_how_long_is_left_and_the_one_command() {
  let words = &CLAUDE_CODE.login;
  let ending = notice(&account("token"), false, Some(90.0 * 60_000.0), None);
  let left = fill_marks(&words.ending_in, &[("left", &fill_marks(&words.hours, &[("n", "2")]))]);

  assert!(ending.starts_with(&fill_marks(&words.head, &[("name", "spare login"), ("when", &left)])));
  assert!(ending.contains(&fill_marks(&words.token_command, &[("path", "/logins/spare")])));

  let soon = notice(&account("directory"), false, Some(10_000.0), None);

  assert!(soon.contains(&fill_marks(&words.minutes, &[("n", "1")])));
  assert!(soon.contains(&fill_marks(&words.directory_command, &[("path", "/logins/spare")])));
  assert!(notice(&account("directory"), true, None, None).contains(&words.gone_after));
}

#[test]
fn the_token_for_the_nodes_is_told_with_the_command_that_makes_it_on_this_machine() {
  let words = &CLAUDE_CODE.node_login;
  let gone = node_notice(&account("directory"), Path::new("/deploy"), NodeToken::Gone, None);

  assert!(gone.starts_with(&fill_marks(&words.head, &[("name", "spare login"), ("when", &words.gone)])));
  assert!(gone.contains(&fill_marks(&words.command, &[("deploy", "/deploy"), ("name", "spare login")])));
  assert!(gone.ends_with(&words.gone_after));

  let ending =
    node_notice(&account("directory"), Path::new("/deploy"), NodeToken::Ending, Some(3.0 * 3_600_000.0));

  assert!(ending.contains(&fill_marks(&CLAUDE_CODE.login.hours, &[("n", "3")])));
  assert!(ending.ends_with(&CLAUDE_CODE.login.ending_after));
  assert!(
    node_notice(&account("directory"), Path::new("/d"), NodeToken::Missing, None).contains(&words.missing)
  );
}

// A directory account that carries on by its token for the work nodes is told so in place of its work moving
// to the next account, with the day that token's own year ends, whether its login has ended or is about to.
#[test]
fn a_notice_says_the_account_carries_on_by_its_token_and_when_that_token_ends() {
  let words = &CLAUDE_CODE.login;
  // 2027-10-10 at noon, UTC.
  let end = 1_823_169_600_000.0;
  let gone = notice(&account("directory"), true, None, Some(end));
  let ending = notice(&account("directory"), false, Some(3.0 * 3_600_000.0), Some(end));

  assert!(gone.ends_with(&words.carried_on.replace("{end}", "2027-10-10")), "{gone}");
  assert!(!gone.contains(&words.gone_after), "{gone}");
  assert!(ending.ends_with(&words.carries_on.replace("{end}", "2027-10-10")), "{ending}");
  assert!(!ending.contains(&words.ending_after), "{ending}");
}

// A token's year is told as each lead before its end is crossed, the nearest lead standing for those it
// passed at once, and not before the furthest.
#[test]
fn a_token_s_year_is_told_at_each_lead_before_its_end() {
  let day = 24.0 * 3_600_000.0;
  let leads = &CLAUDE_CODE.node_login.notice_ahead_days;
  let furthest = *leads.iter().max().unwrap() as f64;
  let nearest = *leads.iter().min().unwrap();

  assert!(furthest >= 30.0, "a token is warned a month ahead at least");
  assert_eq!(node_token_lead((furthest + 1.0) * day), None);
  assert_eq!(node_token_lead(furthest * day), Some(furthest as i64));
  assert_eq!(node_token_lead(0.5 * day), Some(nearest));
  assert_eq!(node_token_lead(-day), Some(nearest));
}

//! An agent's home in herdr: what a pane's command line says stands in it, and a title kept to its width.

use agency_server::places::home::{
  dev_tab_label, dev_worker, head_agent, keeps_standing, runs_pane_head, short_title, standing_command,
};
use policy::place_words::PLACE_WORDS;

fn line(parts: &[&str]) -> Vec<String> {
  parts.iter().map(|part| (*part).to_owned()).collect()
}

#[test]
fn a_panes_command_says_what_stands_in_it() {
  let head = line(&["/bin/sh", "-c", "loop", "bmo-head-worker", "/d/bin/pane-head", "worker"]);
  let bun_head = line(&[
    "/bin/sh",
    "-c",
    "loop",
    "bmo-head-worker",
    "/b/bun",
    "/d/scripts/agent-pane.ts",
    "worker",
  ]);
  let dev = line(&["/d/bin/bmo-agency", "dev-pane", "worker", "--version=abc"]);

  assert_eq!(head_agent(&head), Some("worker"));
  assert!(runs_pane_head(&head) && !runs_pane_head(&bun_head));
  assert_eq!(dev_worker(&dev), Some("worker"));
  assert!(keeps_standing(&dev) && keeps_standing(&standing_command("follow", &line(&["x"]))));
  assert!(!keeps_standing(&line(&["zsh"])));
}

#[test]
fn a_title_keeps_its_first_line_within_its_width() {
  assert_eq!(short_title("\n  second  line\nthird", 48), "second line");
  assert_eq!(short_title("abcdefgh", 5), "abcd…");
}

#[test]
fn the_dev_server_stands_in_the_tab_the_place_words_name() {
  assert_eq!(dev_tab_label(), PLACE_WORDS.dev_tab);
}

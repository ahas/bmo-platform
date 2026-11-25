use agency_server::control::supervisor_settings::{Settings, saved};
use agency_server::lines::chat::denied;
use agency_server::supervisor_limits::{Limits, offered};
use policy::roles::ROLES;
use serde_json::json;

use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;

fn capabilities() -> Vec<String> {
  ["room.tools", "room.chat", "gate.reference", "gate.translate", "brain.lessons", "deploy"]
    .map(str::to_owned)
    .to_vec()
}

#[test]
fn a_supervisor_whose_settings_close_nothing_is_handed_everything_as_before() {
  let open = Limits::open();

  assert_eq!(open.narrowed(capabilities()), capabilities());
  assert_eq!(open.denied(&denied()), denied());
  assert_eq!(open.kept(), None, "and keeps every built-in tool but the denied list");
  assert!(
    ["job", "agency", "explorer", "artifact", "bmo"]
      .iter()
      .all(|server| open.keeps_server(server))
  );
}

#[test]
fn a_brain_switched_off_takes_every_capability_the_brain_is_reached_through_and_nothing_else() {
  let kept = Limits::new(false, true, None).narrowed(capabilities());

  assert!(ROLES.brain.iter().all(|capability| !kept.contains(capability)), "{kept:?}");
  assert_eq!(kept, ["room.tools", "gate.translate", "deploy"].map(str::to_owned).to_vec());
}

#[test]
fn a_reference_switched_off_takes_the_gates_reading_of_the_brain_and_leaves_the_brains_own_tools() {
  let limits = Limits::new(true, false, None);
  let kept = limits.narrowed(capabilities());

  assert!(ROLES.reference.iter().all(|capability| !kept.contains(capability)), "{kept:?}");
  assert_eq!(
    kept,
    ["room.tools", "room.chat", "gate.translate", "brain.lessons", "deploy"]
      .map(str::to_owned)
      .to_vec()
  );
  assert!(limits.brain_on());
}

#[test]
fn a_tool_limit_keeps_only_what_it_names_and_closes_the_servers_and_program_tools_held_under_the_rest() {
  let limits = Limits::new(
    true,
    true,
    Some(["room.chat", "gate.reference", "web.pages"].map(str::to_owned).to_vec()),
  );

  assert_eq!(
    limits.narrowed(capabilities()),
    ["room.chat", "gate.reference"].map(str::to_owned).to_vec()
  );
  assert!(
    !limits.keeps_server("job") && !limits.keeps_server("agency") && !limits.keeps_server("artifact")
  );
  assert!(
    limits.keeps_server("explorer") && limits.keeps_server("bmo"),
    "a server no limit names stays"
  );

  let denied_now = limits.denied(&denied());

  assert!(denied_now.starts_with(&denied()), "the line's own list stands first: {denied_now}");
  assert!(
    ["Bash", "Edit", "Write"]
      .iter()
      .all(|tool| denied_now.split(',').any(|named| named == *tool))
  );
}

#[test]
fn a_limit_may_name_what_the_chat_line_the_bundles_and_its_held_entries_offer() {
  let offered = offered();

  assert!(
    ["room.tools", "room.chat", "brain.lessons", "jobs.file", "program.shell"]
      .iter()
      .all(|capability| offered.iter().any(|one| one == capability))
  );
}

#[tokio::test]
async fn a_save_keeps_the_switches_and_the_tool_limit_and_refuses_a_capability_none_offers() {
  let scratch = Scratch::new().await;

  supervisor(scratch.db(), "supervisor-a", &[]).await;

  let held = Settings::of(scratch.db(), "supervisor-a").await.unwrap();

  assert!(held.brain_on, "a supervisor whose settings never held the switch has its brain on");
  assert_eq!(held.tools_allowed, None, "and holds every tool");

  saved(
    scratch.db(),
    &json!({ "agent": "supervisor-a", "brainOn": false, "toolsAllowed": ["room.chat", "gate.reference"] }),
    "tester",
  )
  .await
  .unwrap();

  let held = Settings::of(scratch.db(), "supervisor-a").await.unwrap();
  let limits = Limits::of(scratch.db(), "supervisor-a").await.unwrap();

  assert!(!held.brain_on);
  assert_eq!(held.tools_allowed, Some(vec!["room.chat".to_owned(), "gate.reference".to_owned()]));
  assert!(!limits.brain_on());
  assert!(
    limits.narrowed(capabilities()).is_empty(),
    "the brain's capabilities go even where the limit names them"
  );

  let refused = saved(
    scratch.db(),
    &json!({ "agent": "supervisor-a", "toolsAllowed": ["root.everything"] }),
    "tester",
  )
  .await;

  assert!(refused.unwrap_err().starts_with("A tool limit is null or a list of capabilities among"));

  saved(
    scratch.db(),
    &json!({ "agent": "supervisor-a", "brainOn": true, "referenceOn": false, "toolsAllowed": null }),
    "tester",
  )
  .await
  .unwrap();

  let limits = Limits::of(scratch.db(), "supervisor-a").await.unwrap();

  assert_eq!(limits, Limits::new(true, false, None), "the saved reference switch is the one read");

  saved(scratch.db(), &json!({ "agent": "supervisor-a", "referenceOn": true }), "tester")
    .await
    .unwrap();

  assert_eq!(Limits::of(scratch.db(), "supervisor-a").await.unwrap(), Limits::open());
}

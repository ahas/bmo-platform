//! The seeds agency's server settles its store with: the account on the machine's own Claude Code login, the
//! base prompts and the roles the policy declares; given once, so a second settling writes nothing more.

use agency_server::seeds::{base_prompt_seeds, deployment, role_seeds};
use harness_providers::Machine;
use policy::base_prompts::BASE_PROMPTS;
use policy::roles::ROLES;
use store::seed::settle;

use crate::support::scratch::Scratch;

fn machine(vars: &[(&str, &str)]) -> Machine {
  Machine::with_env(vars.iter().map(|(name, value)| ((*name).to_owned(), (*value).to_owned())), "/d")
}

#[test]
fn the_account_is_the_machines_own_login_directory() {
  let home = deployment(&machine(&[("HOME", "/home/someone")])).account.unwrap();
  let named = deployment(&machine(&[("HOME", "/home/someone"), ("CLAUDE_CONFIG_DIR", "/logins/main")]))
    .account
    .unwrap();

  assert_eq!(
    (
      home.id.as_str(),
      home.name.as_str(),
      home.credential_kind.as_str(),
      home.credential_path.as_str()
    ),
    ("default", "Default account", "directory", "/home/someone/.claude")
  );
  assert_eq!(named.credential_path, "/logins/main");
}

#[tokio::test]
async fn a_fresh_store_is_given_the_account_and_the_twelve_roles_once() {
  let scratch = Scratch::blank().await;
  let seeds = deployment(&machine(&[("HOME", "/home/someone")]));

  settle(scratch.db(), &seeds).await.unwrap();
  settle(scratch.db(), &seeds).await.unwrap();

  assert_eq!(scratch.count(r#"SELECT count(*) FROM "claude_accounts""#, vec![]).await, 1);
  assert_eq!(scratch.count(r#"SELECT count(*) FROM "roles""#, vec![]).await, 12);
  assert_eq!(scratch.count(r#"SELECT count(*) FROM "role_seeded""#, vec![]).await, 12);
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "role_provider_prompts" WHERE "role" = 'coder' AND "provider" = 'claude-code'
             AND "prompt" <> ''"#,
        vec![]
      )
      .await,
    1,
    "the coder's Claude Code runs are given its provider prompt"
  );
}

#[test]
fn every_declared_base_prompt_is_seeded_with_its_files_text() {
  let code = std::env::current_dir().unwrap();
  let seeds = base_prompt_seeds(&code);

  assert_eq!(seeds.len(), BASE_PROMPTS.seed.len());
  for (seed, declared) in seeds.iter().zip(&BASE_PROMPTS.seed) {
    let file = std::fs::read_to_string(code.join("prompts").join("base").join(&declared.file)).unwrap();

    assert_eq!(
      (seed.name.as_str(), seed.label.as_str()),
      (declared.name.as_str(), declared.label.as_str())
    );
    assert_eq!(seed.text, file);
  }
  assert!(seeds.iter().any(|seed| seed.name == "dsh" && seed.label == "DeepSeek Harness based"));
}

#[test]
fn a_seed_whose_file_cannot_be_read_is_left_out() {
  assert!(base_prompt_seeds(std::path::Path::new("/nowhere")).is_empty());
}

#[test]
fn every_declared_role_is_seeded_with_its_prompt_and_its_description() {
  let code = std::env::current_dir().unwrap();
  let seeds = role_seeds(&code);

  assert_eq!(seeds.len(), ROLES.roles.len());
  for (seed, declared) in seeds.iter().zip(&ROLES.roles) {
    let read = |file: &str| std::fs::read_to_string(code.join("prompts").join("roles").join(file)).unwrap();
    let description = declared.description.clone();
    let prompt = declared.prompt.as_deref().map(read);
    let provider_prompts: Vec<(String, String)> = declared
      .provider_prompts
      .iter()
      .map(|(provider, file)| (provider.clone(), read(file)))
      .collect();

    assert_eq!(
      (&seed.name, &seed.label, &seed.bundles),
      (&declared.name, &declared.label, &declared.bundles)
    );
    assert_eq!(seed.max_concurrent, declared.max_concurrent);
    assert_eq!(seed.line, declared.line);
    assert_eq!(seed.provider_prompts, provider_prompts);
    assert_eq!(seed.prompt, prompt);
    assert_eq!(seed.description, description);
    assert_eq!(seed.sessions, declared.sessions.word());
    assert!(
      seed.description.as_deref().is_some_and(|text| !text.trim().is_empty()),
      "{} is seeded with no description",
      seed.name
    );
  }
  assert!(seeds.iter().any(|seed| seed.name == "producer" && seed.prompt.is_some()));

  // A role whose prompt file cannot be read waits for a settling that can; one naming none is given.
  let elsewhere: Vec<String> =
    role_seeds(std::path::Path::new("/nowhere")).into_iter().map(|seed| seed.name).collect();

  assert_eq!(elsewhere, ["worker", "voyager", "seeker", "transliterator", "upkeep", "note-assistant"]);
}

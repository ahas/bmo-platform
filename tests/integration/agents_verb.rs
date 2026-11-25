//! A repository's support checkout count as `bmo-agency agent --set <repository> --support <n>` writes it
//! (docs/worker-slots.md, 「The base checkout and the support checkouts」), and the counts and names it
//! refuses; and a registration's languages as `bmo-agency agent --register <name> --languages <list>|none`
//! writes them and `--registered` shows them (docs/own-llm.md, 「Where it is set」).

use std::path::Path;

use agency_server::verbs::agents::{AgentsArgs, AgentsHands, Said, Stand, agents};
use clap::Parser;
use futures_util::future::BoxFuture;
use harness_providers::Machine;
use sea_orm::DatabaseConnection;

use crate::integration::common::Scratch;
use crate::support::grounds::ground_on;
use crate::support::rows::repository;

// The verb changes rows alone, so its workbench, worker notice and standing input are never asked.
struct NoHands;

impl AgentsHands for NoHands {
  fn settle<'a>(&'a self, _worker: &'a str) -> BoxFuture<'a, ()> {
    Box::pin(async {})
  }

  fn running(&self) -> BoxFuture<'_, Option<Vec<String>>> {
    Box::pin(async { None })
  }

  fn standing_input(&self) -> String {
    String::new()
  }
}

// One call of the verb, `--set <name> --support <count>`.
async fn ask(db: &DatabaseConnection, home: &Path, name: &str, support: &str) -> Said {
  let args = AgentsArgs {
    set: Some(name.to_owned()),
    support: Some(support.to_owned()),
    ..AgentsArgs::default()
  };

  asked(db, home, &args).await
}

// One call of the verb with the arguments given.
async fn asked(db: &DatabaseConnection, home: &Path, args: &AgentsArgs) -> Said {
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), home.to_string_lossy().into_owned()),
      ("PATH".to_owned(), "/nowhere".to_owned()),
    ],
    home.join("data"),
  );
  let ground = ground_on(db.clone(), home);
  let hands = NoHands;
  let stand = Stand {
    ground: &ground,
    machine: &machine,
    hands: &hands,
    home,
  };

  agents(&stand, args).await.unwrap()
}

// The repository's support count as its row holds it now, none for `auto`.
async fn support_of(scratch: &Scratch, name: &str) -> Option<i64> {
  store::one(
    scratch.db(),
    r#"SELECT "support_count" FROM "repositories" WHERE "name" = $1"#,
    vec![name.into()],
  )
  .await
  .unwrap()
  .and_then(|row| row.try_get::<Option<i64>>("", "support_count").unwrap())
}

#[tokio::test]
async fn a_support_count_given_on_the_verb_is_written_to_the_repository() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  repository(scratch.db(), "video", "/checkouts/video", &[]).await;

  assert_eq!(support_of(&scratch, "video").await, None, "a repository starts at auto");

  for (count, held) in [("3", Some(3)), ("0", Some(0)), ("auto", None)] {
    let said = ask(scratch.db(), home.path(), "video", count).await;

    assert_eq!(said.ended, None, "{count}: {said:?}");
    assert_eq!(support_of(&scratch, "video").await, held, "{count}");
  }
}

#[tokio::test]
async fn a_support_count_outside_the_range_is_refused_and_writes_nothing() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  repository(scratch.db(), "video", "/checkouts/video", &[]).await;

  for count in ["9", "-1", "1.5", "many", "AUTO"] {
    let said = ask(scratch.db(), home.path(), "video", count).await;

    assert_eq!(said.ended.as_ref().map(|(code, _)| *code), Some(1), "{count}: {said:?}");
    assert_eq!(support_of(&scratch, "video").await, None, "{count}");
  }
}

#[tokio::test]
async fn a_name_that_is_no_repository_is_refused() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  let said = ask(scratch.db(), home.path(), "nowhere", "1").await;

  assert_eq!(said.ended.as_ref().map(|(code, _)| *code), Some(1), "{said:?}");
}

#[tokio::test]
async fn a_registration_s_languages_are_given_on_the_verb_and_listed() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let verb = |words: &[&str]| AgentsArgs::try_parse_from([&["agent"], words].concat()).unwrap();

  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
            "thinking", "walked", "added_at", "keeps_research")
         VALUES ('small', 'Small', 'served', 100000, 4000, 0, 0, 1, 0)"#,
      vec![],
    )
    .await;

  let said = asked(
    scratch.db(),
    home.path(),
    &verb(&["--register", "small", "--languages", "English,Japanese"]),
  )
  .await;

  assert_eq!(said.ended, None, "{said:?}");
  assert!(
    said.out.iter().any(|line| line == "  writes results in English and Japanese alone"),
    "{said:?}"
  );

  let said = asked(scratch.db(), home.path(), &verb(&["--register", "small", "--languages", "none"])).await;

  assert_eq!(said.ended, None, "{said:?}");
  assert!(!said.out.iter().any(|line| line.contains("writes results in")), "{said:?}");
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "registered_models" WHERE "name" = 'small' AND "languages" IS NULL"#,
        vec![],
      )
      .await,
    1
  );
}

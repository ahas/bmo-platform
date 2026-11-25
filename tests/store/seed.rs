//! The seeds a store is given as it is settled (docs/store.md, 「Its seeds」): each written once, and a
//! row the owner removed left removed.

use store::seed::{AccountSeed, BasePromptSeed, CatalogueSeed, RoleSeed, Seeds, settle};

use crate::support::scratch::Scratch;

fn role(name: &str, cap: i64, sessions: &str, prompt: Option<&str>) -> RoleSeed {
  RoleSeed {
    name: name.into(),
    label: name.to_uppercase(),
    bundles: vec!["room.send".into()],
    line: "worker".into(),
    order: r#"[{"provider":"deepseek","model":"deepseek-flash"}]"#.into(),
    max_concurrent: cap,
    prompt: prompt.map(str::to_owned),
    description: Some(format!("What {name} is for.")),
    sessions: sessions.into(),
    fold_ratio: 0.5,
    provider_prompts: if name == "maker" {
      vec![("claude-code".into(), "Use your subagents.".into())]
    } else {
      vec![]
    },
    follows_asker: false,
  }
}

fn seeds() -> Seeds {
  Seeds {
    account: Some(AccountSeed {
      id: "default".into(),
      name: "Default account".into(),
      credential_kind: "directory".into(),
      credential_path: "$HOME/.claude".into(),
    }),
    catalogue: vec![CatalogueSeed {
      name: "entry".into(),
      label: "Entry".into(),
      file: "entry.gguf".into(),
      bytes: 1_024,
      sha256: "00".repeat(32),
      source: Some("https://example.com/entry.gguf".into()),
      engine: Some("engine".into()),
      registration: Some("engine".into()),
      thinking: Some(false),
      temperature: Some(0.7),
      top_p: None,
      presence_penalty: None,
      max_tokens: Some(512),
      note: None,
    }],
    base_prompts: vec![
      BasePromptSeed {
        name: "harness".into(),
        label: "Harness based".into(),
        text: "You are a harness.".into(),
        agents: vec!["worker".into()],
      },
      BasePromptSeed {
        name: "other".into(),
        label: "Other based".into(),
        text: "You are another harness.".into(),
        agents: vec![],
      },
    ],
    roles: vec![role("talker", 3, "keep", None), role("maker", 0, "fresh", Some("Make things."))],
  }
}

async fn role_row(scratch: &Scratch, name: &str) -> Option<sea_orm::QueryResult> {
  store::one(scratch.db(), r#"SELECT * FROM "roles" WHERE "name" = $1"#, vec![name.into()])
    .await
    .unwrap()
}

#[tokio::test]
async fn a_role_is_given_once_with_its_order_cap_line_threshold_and_provider_prompts() {
  let scratch = Scratch::blank().await;

  settle(scratch.db(), &seeds()).await.unwrap();

  let talker = role_row(&scratch, "talker").await.unwrap();
  let maker = role_row(&scratch, "maker").await.unwrap();

  assert_eq!(talker.try_get::<i64>("", "max_concurrent").unwrap(), 3);
  assert_eq!(talker.try_get::<String>("", "line").unwrap(), "worker");
  assert_eq!(talker.try_get::<String>("", "sessions").unwrap(), "keep");
  assert_eq!(talker.try_get::<f64>("", "fold_ratio").unwrap(), 0.5);
  assert!(talker.try_get::<String>("", "provider_order").unwrap().contains("deepseek-flash"));
  assert_eq!(maker.try_get::<i64>("", "max_concurrent").unwrap(), 0, "a cap of zero closes a role");
  assert_eq!(maker.try_get::<Option<String>>("", "prompt").unwrap().as_deref(), Some("Make things."));
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "role_provider_prompts" WHERE "role" = 'maker' AND "provider" = 'claude-code'"#,
        vec![]
      )
      .await,
    1
  );
  assert_eq!(talker.try_get::<i64>("", "position").unwrap(), 0);
  assert_eq!(maker.try_get::<i64>("", "position").unwrap(), 1);
}

#[tokio::test]
async fn what_the_owner_removed_stays_removed_and_what_the_owner_set_stands() {
  let scratch = Scratch::blank().await;

  settle(scratch.db(), &seeds()).await.unwrap();
  scratch
    .run(r#"UPDATE "roles" SET "max_concurrent" = 9 WHERE "name" = 'talker'"#, vec![])
    .await;
  scratch.run(r#"DELETE FROM "roles" WHERE "name" = 'maker'"#, vec![]).await;
  scratch
    .run(r#"DELETE FROM "role_provider_prompts" WHERE "role" = 'maker'"#, vec![])
    .await;
  settle(scratch.db(), &seeds()).await.unwrap();

  assert_eq!(
    scratch.count(r#"SELECT count(*) FROM "role_provider_prompts""#, vec![]).await,
    0,
    "a provider prompt the owner took away stays away"
  );

  assert_eq!(
    role_row(&scratch, "talker")
      .await
      .unwrap()
      .try_get::<i64>("", "max_concurrent")
      .unwrap(),
    9
  );
  assert!(role_row(&scratch, "maker").await.is_none(), "a role the owner removed stays removed");
}

#[tokio::test]
async fn a_store_holding_no_account_is_given_the_seed_again() {
  let scratch = Scratch::blank().await;

  settle(scratch.db(), &seeds()).await.unwrap();
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  settle(scratch.db(), &seeds()).await.unwrap();

  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "claude_accounts" WHERE "id" = 'default'"#, vec![])
      .await,
    1
  );
}

async fn prompt_text(scratch: &Scratch, name: &str) -> Option<String> {
  store::one(
    scratch.db(),
    r#"SELECT "text" FROM "base_prompts" WHERE "name" = $1"#,
    vec![name.into()],
  )
  .await
  .unwrap()
  .map(|row| row.try_get::<String>("", "text").unwrap())
}

async fn choice(scratch: &Scratch, agent: &str) -> Option<String> {
  store::one(
    scratch.db(),
    r#"SELECT "prompt" FROM "agent_base_prompts" WHERE "agent" = $1"#,
    vec![agent.into()],
  )
  .await
  .unwrap()
  .map(|row| row.try_get::<String>("", "prompt").unwrap())
}

#[tokio::test]
async fn a_base_prompt_declared_later_reaches_a_standing_store_once() {
  let scratch = Scratch::blank().await;
  let mut before = seeds();

  before.base_prompts.clear();
  settle(scratch.db(), &before).await.unwrap();
  assert_eq!(prompt_text(&scratch, "harness").await, None);

  settle(scratch.db(), &seeds()).await.unwrap();
  assert_eq!(prompt_text(&scratch, "harness").await.as_deref(), Some("You are a harness."));
  assert_eq!(prompt_text(&scratch, "other").await.as_deref(), Some("You are another harness."));
  assert_eq!(choice(&scratch, "worker").await.as_deref(), Some("harness"));

  scratch
    .run(
      r#"UPDATE "base_prompts" SET "text" = 'The owner''s own.' WHERE "name" = 'harness'"#,
      vec![],
    )
    .await;
  scratch.run(r#"DELETE FROM "base_prompts" WHERE "name" = 'other'"#, vec![]).await;
  scratch.run(r#"DELETE FROM "agent_base_prompts""#, vec![]).await;
  settle(scratch.db(), &seeds()).await.unwrap();

  assert_eq!(prompt_text(&scratch, "harness").await.as_deref(), Some("The owner's own."));
  assert_eq!(prompt_text(&scratch, "other").await, None);
  assert_eq!(choice(&scratch, "worker").await, None);
}

#[tokio::test]
async fn a_prompt_already_kept_under_a_seeds_name_is_left_and_gives_no_choice() {
  let scratch = Scratch::blank().await;
  let mut before = seeds();

  before.base_prompts.clear();
  settle(scratch.db(), &before).await.unwrap();
  scratch
    .run(
      r#"INSERT INTO "base_prompts" ("name", "label", "text") VALUES ('harness', 'Mine', 'Kept before.')"#,
      vec![],
    )
    .await;
  settle(scratch.db(), &seeds()).await.unwrap();

  assert_eq!(prompt_text(&scratch, "harness").await.as_deref(), Some("Kept before."));
  assert_eq!(choice(&scratch, "worker").await, None);
}

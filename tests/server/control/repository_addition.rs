//! `bmo-agency repository add` (docs/repositories.md, 「What a repository row holds」): a registration
//! written through the server's own route, the row printed as `bmo agents` prints one, and the route's
//! refusals carried back to the caller; nothing on disk is touched.

use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::support::scratch::Scratch;
use crate::support::{binary, standing};

const TOKEN: &str = "secret";

fn add(port: u16, home: &Path, data: &Path, words: &[&str]) -> Output {
  Command::new(binary::path("bmo-agency"))
    .arg("repository")
    .arg("add")
    .args(words)
    .env_clear()
    .env("HOME", home)
    .env("PATH", std::env::var("PATH").unwrap_or_else(|_| "/usr/bin:/bin".to_owned()))
    .env("BMO_DATA_DIR", data)
    .env("AGENCY_CONTROL_PORT", port.to_string())
    .env("AGENCY_CONTROL_TOKEN", TOKEN)
    .stdin(Stdio::null())
    .output()
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_repository_is_added_through_the_route_and_its_refusals_are_carried_back() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();
  let server = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);

  // The row is written, and printed as `bmo agents` prints a repository.
  let added = add(
    server.port,
    home.path(),
    &data,
    &["repo", "--path", "/checkouts/repo", "--label", "Repo"],
  );

  assert!(added.status.success(), "{}", String::from_utf8_lossy(&added.stderr));
  assert!(
    String::from_utf8_lossy(&added.stdout).contains("repository repo: /checkouts/repo"),
    "{}",
    String::from_utf8_lossy(&added.stdout)
  );

  let row = store::one(
    scratch.db(),
    r#"SELECT "path", "label" FROM "repositories" WHERE "name" = 'repo'"#,
    vec![],
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(row.try_get::<String>("", "path").unwrap(), "/checkouts/repo");
  assert_eq!(row.try_get::<Option<String>>("", "label").unwrap().as_deref(), Some("Repo"));

  // A name already taken is refused in the route's words, and nothing is written beside it.
  let taken = add(server.port, home.path(), &data, &["repo", "--path", "/checkouts/other"]);

  assert!(!taken.status.success(), "a taken name is refused");
  assert!(
    String::from_utf8_lossy(&taken.stderr).contains("The name repo is taken."),
    "{}",
    String::from_utf8_lossy(&taken.stderr)
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "repositories" WHERE "name" = 'repo'"#, vec![])
      .await,
    1
  );

  // A name outside the route's shape is refused in the route's words.
  let shaped = add(server.port, home.path(), &data, &["Bad_Name", "--path", "/checkouts/bad"]);

  assert!(!shaped.status.success(), "an ill-shaped name is refused");
  assert!(
    String::from_utf8_lossy(&shaped.stderr)
      .contains("A repository's name is lowercase letters, digits and hyphens."),
    "{}",
    String::from_utf8_lossy(&shaped.stderr)
  );

  // A repository names its path: an empty one is refused in the route's words, and no row is written.
  let pathless = add(server.port, home.path(), &data, &["pathless", "--path", ""]);

  assert!(!pathless.status.success(), "a repository without a path is refused");
  assert!(
    String::from_utf8_lossy(&pathless.stderr).contains("A repository names its path."),
    "{}",
    String::from_utf8_lossy(&pathless.stderr)
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "repositories" WHERE "name" = 'pathless'"#, vec![])
      .await,
    0
  );

  // An umbrella is added with no support checkouts, and a change that would give it some is refused.
  let umbrella = add(server.port, home.path(), &data, &["tree", "--path", "/checkouts", "--umbrella"]);

  assert!(umbrella.status.success(), "{}", String::from_utf8_lossy(&umbrella.stderr));

  let row = store::one(
    scratch.db(),
    r#"SELECT "umbrella", "support_count" FROM "repositories" WHERE "name" = 'tree'"#,
    vec![],
  )
  .await
  .unwrap()
  .unwrap();

  assert_eq!(row.try_get::<i64>("", "umbrella").unwrap(), 1);
  assert_eq!(row.try_get::<Option<i64>>("", "support_count").unwrap(), Some(0));

  let changed = reqwest::Client::new()
    .patch(format!("http://127.0.0.1:{}/repositories/tree", server.port))
    .bearer_auth(TOKEN)
    .json(&serde_json::json!({ "supportCount": 2 }))
    .send()
    .await
    .unwrap();
  let status = changed.status().as_u16();
  let refused: serde_json::Value = changed.json().await.unwrap();

  assert_eq!(
    (status, refused["field"].as_str().or(refused["code"].as_str())),
    (400, Some("umbrella")),
    "{refused}"
  );
}

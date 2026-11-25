//! `bmo-agency repository remove` (docs/repositories.md, 「What a repository row holds」): a registration
//! taken away through the server's own route, and the route's refusal while a job holds one of its checkouts
//! carried back to the caller; nothing on disk is touched.

use std::path::Path;
use std::process::{Command, Output, Stdio};

use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::{binary, standing};

const TOKEN: &str = "secret";

fn remove(port: u16, home: &Path, data: &Path, name: &str) -> Output {
  Command::new(binary::path("bmo-agency"))
    .args(["repository", "remove", name])
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
async fn a_repository_is_removed_through_the_route_and_refused_while_a_job_holds_a_checkout() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  std::fs::create_dir_all(&data).unwrap();
  repository(scratch.db(), "free", "/checkouts/free", &[]).await;
  repository(scratch.db(), "held", "/checkouts/held", &[]).await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name",
                             "state", "started_at", "prompt")
         VALUES (1, 20, 7, 'harness', 1, 'held', 'running', $1, 'work')"#,
      vec![agency_server::now_ms().into()],
    )
    .await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), TOKEN);

  // A job holds the repository's base checkout, so its registration stays.
  let refused = remove(server.port, home.path(), &data, "held");

  assert!(!refused.status.success(), "a held repository is not removed");
  assert!(
    String::from_utf8_lossy(&refused.stderr).contains("A job holds held, a checkout of held."),
    "{}",
    String::from_utf8_lossy(&refused.stderr)
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "repositories" WHERE "name" = 'held'"#, vec![])
      .await,
    1
  );

  // One held by no job goes, and a second removal says there is nothing to remove.
  let removed = remove(server.port, home.path(), &data, "free");

  assert!(removed.status.success(), "{}", String::from_utf8_lossy(&removed.stderr));
  assert!(
    String::from_utf8_lossy(&removed.stdout).contains("removed free"),
    "{}",
    String::from_utf8_lossy(&removed.stdout)
  );
  assert_eq!(
    scratch
      .count(r#"SELECT COUNT(*) FROM "repositories" WHERE "name" = 'free'"#, vec![])
      .await,
    0
  );

  let gone = remove(server.port, home.path(), &data, "free");

  assert!(!gone.status.success());
  assert!(
    String::from_utf8_lossy(&gone.stderr).contains("There is no repository free."),
    "{}",
    String::from_utf8_lossy(&gone.stderr)
  );
}

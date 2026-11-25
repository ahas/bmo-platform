//! Checkouts lent to a job (docs/worker-slots.md, 「A checkout lent to a job」, 「Loans across a job's review」):
//! a launch lending each repository its item names on the job's branch and naming them in its prompt, a launch
//! waiting while one is busy and going on once it frees, the loans standing through a review round, a paused job
//! parking them and taking them again, the bound of checkouts one job holds, and `bmo job borrow --wait` waiting
//! for a checkout instead of being refused.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use agency_server::control::loans::{Borrower, Lending, lend_job};
use agency_server::control::work::{launched, publish};
use agency_server::now_ms;
use agency_server::places::loan_branch::standing_branch;
use agency_server::queue::holds::release_ended;
use agency_server::queue::workers::workers;
use policy::workers::WORKERS;
use serde_json::json;

use crate::support::git::{at, git};
use crate::support::grounds::seeded_with;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::stand_in::StandIn;
use crate::support::{binary, standing};

// A repository of one commit on main, committing under a neutral author.
fn made_repository(path: &Path) {
  std::fs::create_dir_all(path).unwrap();

  let here = at(path);

  git(&["-C", &here, "init", "--quiet", "--initial-branch", "main"]);

  for (key, value) in [
    ("user.name", "tester"),
    ("user.email", "tester@example.com"),
    ("commit.gpgsign", "false"),
  ] {
    git(&["-C", &here, "config", key, value]);
  }

  std::fs::write(path.join("README.md"), "base\n").unwrap();
  git(&["-C", &here, "add", "."]);
  git(&["-C", &here, "commit", "--quiet", "--message", "base"]);
}

// A job working in the checkout `helper`, of the item given, in the state given, its item naming the repositories
// to be lent.
async fn job(scratch: &Scratch, id: i64, item: i64, state: &str, named: Option<&str>) {
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES ($1, $2, 'harness', 'helper', 'running', $3, $3)"#,
      vec![id.into(), item.into(), now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "run_name", "started_at", "prompt", "item_agent", "named_repositories")
         VALUES ($1, $2, 1, 'harness', $1, 'helper', $3, $4, $5, $6, 'worker-01', $7)"#,
      vec![
        id.into(),
        item.into(),
        state.into(),
        format!("job-{id}").into(),
        now.into(),
        format!("# item {item}\n\nDo the work.").into(),
        named.map(str::to_owned).into(),
      ],
    )
    .await;
}

// Another job at work in the checkout given, which it holds.
async fn holding(scratch: &Scratch, id: i64, checkout: &str) {
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES ($1, $1, 'harness', $2, 'running', $3, $3)"#,
      vec![id.into(), checkout.into(), now.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "started_at", "prompt")
         VALUES ($1, $1, 1, 'harness', $1, $2, 'running', $3, 'other work')"#,
      vec![id.into(), checkout.into(), now.into()],
    )
    .await;
}

// The checkouts the job's loans hold now.
async fn lent_to(scratch: &Scratch, id: i64) -> Vec<String> {
  store::all(
    scratch.db(),
    r#"SELECT "worker_name" FROM "assignments" WHERE "holder_name" = $1 AND "state" = 'held' ORDER BY "worker_name""#,
    vec![format!("job:{id}").into()],
  )
  .await
  .unwrap()
  .iter()
  .map(|row| row.try_get("", "worker_name").unwrap())
  .collect()
}

async fn held_for(scratch: &Scratch, id: i64) -> Option<String> {
  store::one(scratch.db(), r#"SELECT "held_for" FROM "jobs" WHERE "id" = $1"#, vec![id.into()])
    .await
    .unwrap()
    .unwrap()
    .try_get("", "held_for")
    .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_lends_each_repository_its_item_names_on_the_job_s_branch_and_names_them_in_its_prompt() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let (web, ui) = (home.path().join("web"), home.path().join("ui"));

  // The job's own checkout is a plain base on main, so its prompt carries no bundle above the lent checkouts.
  made_repository(&home.path().join("helper"));
  made_repository(&web);
  made_repository(&ui);
  repository(scratch.db(), "helper", &at(&home.path().join("helper")), &[]).await;
  repository(scratch.db(), "web", &at(&web), &[("support_count", 0i64.into())]).await;
  repository(scratch.db(), "ui", &at(&ui), &[("support_count", 1i64.into())]).await;
  job(&scratch, 21, 210, "starting", Some("web,ui")).await;

  let sessions = StandIn::answering(&[("Done.", "s-1")]);

  launched(&ground, Some(sessions.clone()), 21, None)
    .await
    .unwrap()
    .expect("the turn's end");

  // The base checkout of the one, and the other's support worktree, made for the loan, each on the job's branch.
  let support = format!("{}-w2", at(&ui));

  assert_eq!(lent_to(&scratch, 21).await, vec!["ui-w2".to_owned(), "web".to_owned()]);
  assert_eq!(standing_branch(&at(&web)).as_deref(), Some("work/210"));
  assert_eq!(standing_branch(&support).as_deref(), Some("work/210"));

  // The turn's prompt opens, under its title, with each lent repository, its path and its branch, and the job
  // keeps that prompt.
  let turns = sessions.turns();
  let prompt = &turns[0].2;

  assert!(prompt.starts_with("# item 210\n\n## Checkouts lent to this job\n"), "{prompt}");
  assert!(prompt.contains(&format!("- `web`: `{}`, on `work/210`", at(&web))), "{prompt}");
  assert!(prompt.contains(&format!("- `ui`: `{support}`, on `work/210`")), "{prompt}");

  let kept: String = store::one(scratch.db(), r#"SELECT "prompt" FROM "jobs" WHERE "id" = 21"#, vec![])
    .await
    .unwrap()
    .unwrap()
    .try_get("", "prompt")
    .unwrap();

  assert!(kept.contains("## Checkouts lent to this job"), "{kept}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_waits_while_a_repository_it_names_is_busy_taking_none_and_goes_on_once_it_frees() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = Arc::new(seeded_with(scratch.db(), home.path(), &Default::default()).await);
  let (web, ui) = (home.path().join("web"), home.path().join("ui"));

  made_repository(&web);
  made_repository(&ui);
  repository(scratch.db(), "helper", &at(&home.path().join("helper")), &[]).await;
  repository(scratch.db(), "web", &at(&web), &[("support_count", 0i64.into())]).await;
  repository(scratch.db(), "ui", &at(&ui), &[("support_count", 0i64.into())]).await;
  job(&scratch, 22, 220, "starting", Some("ui,web")).await;
  holding(&scratch, 30, "web").await;

  let lending = {
    let ground = ground.clone();

    tokio::spawn(async move { lend_job(&ground, 22).await })
  };

  // The job waits, named for what it waits on, and holds none of its loans meanwhile.
  let mut waits = None;

  for _ in 0..400 {
    waits = held_for(&scratch, 22).await;

    if waits.is_some() {
      break;
    }

    tokio::time::sleep(Duration::from_millis(25)).await;
  }

  assert!(waits.as_deref().is_some_and(|why| why.contains("web")), "{waits:?}");
  assert!(
    lent_to(&scratch, 22).await.is_empty(),
    "a launch that waits holds nothing it could not take whole"
  );

  // The job holding the checkout ends; its end wakes the wait, which lends both.
  scratch.run(r#"UPDATE "jobs" SET "state" = 'ended' WHERE "id" = 30"#, vec![]).await;
  publish(&ground, "job.ended", json!({ "id": 30 }));

  let lent = tokio::time::timeout(Duration::from_secs(30), lending)
    .await
    .expect("the launch goes on once the checkout frees")
    .unwrap()
    .unwrap()
    .expect("lent");

  assert_eq!(lent.len(), 2);
  assert_eq!(lent_to(&scratch, 22).await, vec!["ui".to_owned(), "web".to_owned()]);
  assert_eq!(held_for(&scratch, 22).await, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn loans_stand_through_a_review_round_and_a_paused_job_parks_them_and_takes_the_same_ones_again() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded_with(scratch.db(), home.path(), &Default::default()).await;
  let (web, ui) = (home.path().join("web"), home.path().join("ui"));

  made_repository(&web);
  made_repository(&ui);
  repository(scratch.db(), "helper", &at(&home.path().join("helper")), &[]).await;
  repository(scratch.db(), "web", &at(&web), &[("support_count", 0i64.into())]).await;
  repository(scratch.db(), "ui", &at(&ui), &[("support_count", 1i64.into())]).await;
  job(&scratch, 23, 230, "running", Some("web,ui")).await;

  lend_job(&ground, 23).await.unwrap().expect("lent");

  let support = format!("{}-w2", at(&ui));

  std::fs::write(web.join("notes.md"), "the job's work\n").unwrap();
  std::fs::write(Path::new(&support).join("README.md"), "changed\n").unwrap();

  // A review round keeps every loan.
  scratch.run(r#"UPDATE "jobs" SET "state" = 'reviewing' WHERE "id" = 23"#, vec![]).await;
  release_ended(scratch.db()).await.unwrap();

  assert_eq!(lent_to(&scratch, 23).await, vec!["ui-w2".to_owned(), "web".to_owned()]);

  // Paused, the job gives them back: each parked on its branch, the base put back on main, the worktree left on
  // the branch.
  scratch.run(r#"UPDATE "jobs" SET "review_parked" = true WHERE "id" = 23"#, vec![]).await;
  release_ended(scratch.db()).await.unwrap();

  assert!(lent_to(&scratch, 23).await.is_empty());
  assert_eq!(standing_branch(&at(&web)).as_deref(), Some("main"));
  assert!(!web.join("notes.md").exists(), "the work is parked off main");
  assert!(git(&["-C", &at(&web), "log", "-1", "--format=%s", "work/230"]).starts_with("Park "));
  assert_eq!(standing_branch(&support).as_deref(), Some("work/230"));
  assert_eq!(git(&["-C", &support, "status", "--porcelain"]), "");

  // Taken up again, it is lent the same worktree, which still stands on its branch, and the base again on the
  // branch its work was parked on.
  scratch
    .run(
      r#"UPDATE "jobs" SET "review_parked" = false, "state" = 'running' WHERE "id" = 23"#,
      vec![],
    )
    .await;

  let lent = lend_job(&ground, 23).await.unwrap().expect("lent again");

  assert!(lent.iter().all(|lent| lent.fresh));
  assert_eq!(lent_to(&scratch, 23).await, vec!["ui-w2".to_owned(), "web".to_owned()]);
  assert_eq!(standing_branch(&at(&web)).as_deref(), Some("work/230"));
  assert_eq!(std::fs::read_to_string(web.join("notes.md")).unwrap(), "the job's work\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_job_is_lent_no_checkout_past_its_bound() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded_with(scratch.db(), home.path(), &Default::default()).await;

  repository(scratch.db(), "helper", &at(&home.path().join("helper")), &[]).await;
  repository(scratch.db(), "web", &at(&home.path().join("web")), &[("support_count", 0i64.into())]).await;
  job(&scratch, 24, 240, "running", None).await;

  // Its own checkout and every loan but one already held.
  for loan in 0..WORKERS.job_checkouts_most - 1 {
    scratch
      .run(
        r#"INSERT INTO "assignments" ("item_id", "holder_name", "worker_name", "state", "created_at", "changed_at")
           VALUES (240, 'job:24', $1, 'held', 0, 0)"#,
        vec![format!("held-{loan}").into()],
      )
      .await;
  }

  let borrower = Borrower::of_job(&ground, 24).await.unwrap().unwrap();
  let known = workers(scratch.db(), &ground.source).await.unwrap();
  let refused = match borrower.lend(&ground, &known, "web").await {
    Err(refused) => refused,
    Ok(Lending::Lent(lent)) => panic!("lent {} past the bound", lent.checkout.name),
    Ok(Lending::Busy(why)) => panic!("busy rather than refused: {why}"),
  };

  assert_eq!(refused.code.as_deref(), Some("too-many-checkouts"), "{}", refused.error);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_checkout_standing_on_main_with_something_open_is_somebody_s_work_and_is_not_lent() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = seeded_with(scratch.db(), home.path(), &Default::default()).await;
  let web = home.path().join("web");

  made_repository(&web);
  repository(scratch.db(), "helper", &at(&home.path().join("helper")), &[]).await;
  repository(scratch.db(), "web", &at(&web), &[("support_count", 0i64.into())]).await;
  job(&scratch, 25, 250, "running", None).await;
  std::fs::write(web.join("draft.md"), "a person's edit\n").unwrap();

  let borrower = Borrower::of_job(&ground, 25).await.unwrap().unwrap();
  let known = workers(scratch.db(), &ground.source).await.unwrap();

  assert!(matches!(borrower.lend(&ground, &known, "web").await, Ok(Lending::Busy(_))));
  assert_eq!(standing_branch(&at(&web)).as_deref(), Some("main"));

  std::fs::remove_file(web.join("draft.md")).unwrap();

  assert!(matches!(borrower.lend(&ground, &known, "web").await, Ok(Lending::Lent(_))));
}

// A process the test started, taken down where the test ends before it does.
struct Started(std::process::Child);

impl Drop for Started {
  fn drop(&mut self) {
    let _ = self.0.kill();
    let _ = self.0.wait();
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_borrowing_that_waits_says_what_it_waits_for_and_is_lent_once_the_checkout_frees() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";
  let other = home.path().join("other");

  std::fs::create_dir_all(&data).unwrap();
  made_repository(&other);
  repository(scratch.db(), "helper", &at(&home.path().join("helper")), &[]).await;
  repository(scratch.db(), "other", &at(&other), &[("support_count", 0i64.into())]).await;

  // The server answers a waiting borrowing within a tenth of a second, so the verb says it still waits at once.
  let server = standing::stand_server_with(
    home.path(),
    &data,
    &scratch.address(),
    token,
    &[("BMO_BORROW_WAIT_MS", "100")],
  );

  // The jobs run from here on: one standing as the server starts, with no run behind it, is reaped.
  job(&scratch, 3, 41, "running", None).await;
  holding(&scratch, 4, "other").await;

  let mut borrowing = std::process::Command::new(binary::path("bmo-agency"));

  borrowing
    .env_clear()
    .env("PATH", std::env::var("PATH").unwrap_or_default())
    .env("HOME", home.path())
    .env("BMO_DATA_DIR", &data)
    .env("BMO_RUN_NAME", "job-3")
    .env("AGENCY_CONTROL_TOKEN", token)
    .env("AGENCY_CONTROL_PORT", server.port.to_string())
    .args(["job", "borrow", "other", "--wait"])
    .stdin(std::process::Stdio::null())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());

  let mut started = Started(borrowing.spawn().unwrap());
  let out = started.0.stdout.take().unwrap();
  let (said, lines) = std::sync::mpsc::channel::<String>();

  std::thread::spawn(move || {
    use std::io::BufRead;

    for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
      if said.send(line).is_err() {
        break;
      }
    }
  });

  let first = lines.recv_timeout(Duration::from_secs(30)).expect("a line from the waiting borrowing");

  assert!(first.contains("waiting for a checkout of other"), "{first}");

  // The job holding the checkout ends, and the next ask is lent it, on the borrowing job's branch.
  scratch.run(r#"UPDATE "jobs" SET "state" = 'ended' WHERE "id" = 4"#, vec![]).await;

  let mut lent = None;

  while let Ok(line) = lines.recv_timeout(Duration::from_secs(30)) {
    if line.contains("is this job's until it ends") {
      lent = Some(line);
      break;
    }
  }

  assert!(lent.as_deref().is_some_and(|line| line.contains(&at(&other))), "{lent:?}");

  let mut ended = None;

  for _ in 0..600 {
    ended = started.0.try_wait().unwrap();

    if ended.is_some() {
      break;
    }

    std::thread::sleep(Duration::from_millis(50));
  }

  assert_eq!(ended.and_then(|status| status.code()), Some(0));
  assert_eq!(standing_branch(&at(&other)).as_deref(), Some("work/41"));
}

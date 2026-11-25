//! The preamble bundle a job's prompt carries under its title (docs/worker-bench.md, 「Preamble」, 「The bundle is
//! agency's header plus the owner's text」): carried only where something is to be told, the header's lines each
//! said where it holds, the owner's text below it, and a launch reading it afresh from the row and the checkout
//! without keeping it on the job's row; a git checkout, a clone or a worktree alike, told with where it stands
//! and its origin.

use std::path::{Path, PathBuf};

use agency_server::control::preamble::{Bench, Head, Here, Tree};
use agency_server::control::work::launched;
use agency_server::now_ms;
use policy::fill_marks;
use policy::job_prompt::JOB_PROMPT;

use crate::support::git::{at, git};
use crate::support::grounds::seeded_with;
use crate::support::rows::repository;
use crate::support::scratch::Scratch;
use crate::support::stand_in::StandIn;

// A git checkout standing on the branch, its origin the example address.
fn on(branch: &str) -> Tree {
  Tree::Git {
    head: Head::Branch(branch.to_owned()),
    origin: Some("https://example.com/shop.git".to_owned()),
  }
}

// A base checkout on this machine, a git checkout on main, with no preamble and no dev server.
fn plain() -> Bench {
  Bench {
    checkout: "shop".to_owned(),
    repository: "shop".to_owned(),
    path: "/checkouts/shop".to_owned(),
    item: 42,
    support: None,
    tree: on("main"),
    here: Some(Here {
      workbench: PathBuf::from("/data/workbench/shop"),
      dev: None,
    }),
    preamble: None,
  }
}

#[test]
fn a_plain_base_checkout_carries_no_bundle() {
  assert_eq!(plain().words(), "");
}

#[test]
fn a_support_checkout_on_the_item_s_branch_is_told_each_line_in_order_and_the_owner_s_text_last() {
  let bench = Bench {
    checkout: "shop-w2".to_owned(),
    path: "/checkouts/shop-w2".to_owned(),
    support: Some(2),
    tree: on("work/42"),
    here: Some(Here {
      workbench: PathBuf::from("/data/workbench/shop-w2"),
      dev: Some(PathBuf::from("/deploy/bin/bmo-agency")),
    }),
    preamble: Some("The console stands on port 3000.\n".to_owned()),
    ..plain()
  };
  let words = bench.words();
  let order = [
    JOB_PROMPT.bench.heading.clone(),
    "This job runs in `shop-w2`".to_owned(),
    "support checkout 2 of `shop`".to_owned(),
    "`/checkouts/shop-w2` is a git checkout on the branch `work/42`; its origin is `https://example.com/shop.git`"
      .to_owned(),
    "the item's branch `work/42`".to_owned(),
    "`/data/workbench/shop-w2/items/42/`".to_owned(),
    "`bmo_item_42_`".to_owned(),
    "`/deploy/bin/bmo-agency dev-server start shop-w2`".to_owned(),
    "`/deploy/bin/bmo-agency dev-server restart shop-w2`".to_owned(),
    "`/deploy/bin/bmo-agency dev-server stop shop-w2`".to_owned(),
    "`/data/workbench/shop-w2/dev-server.log`".to_owned(),
    format!("{}\n\nThe console stands on port 3000.\n", JOB_PROMPT.bench.preamble_heading),
  ];
  let mut from = 0;

  for part in order {
    let found = words[from..].find(&part).unwrap_or_else(|| panic!("{part} after {from} in {words}"));

    from += found + part.len();
  }

  assert!(words.ends_with("port 3000.\n"), "{words}");
}

#[test]
fn a_directory_that_is_no_git_checkout_is_told_so_and_where_its_workbench_is() {
  let bench = Bench {
    tree: Tree::Plain,
    ..plain()
  };
  let words = bench.words();

  assert!(
    words.contains(&fill_marks(&JOB_PROMPT.bench.not_checkout, &[("path", "/checkouts/shop")])),
    "{words}"
  );
  assert!(words.contains("`/data/workbench/shop`"), "{words}");
  assert!(!words.contains("is a git checkout"), "{words}");
  assert!(!words.contains(&JOB_PROMPT.bench.preamble_heading), "{words}");
}

#[test]
fn a_detached_git_checkout_with_no_origin_is_told_its_commit_and_that_it_has_none() {
  let bench = Bench {
    tree: Tree::Git {
      head: Head::Detached("abc1234".to_owned()),
      origin: None,
    },
    support: Some(2),
    ..plain()
  };
  let words = bench.words();

  assert!(
    words.contains("`/checkouts/shop` is a git checkout detached at commit `abc1234`; it has no origin."),
    "{words}"
  );
  assert!(!words.contains("the item's branch"), "{words}");
}

#[test]
fn a_checkout_on_a_work_node_is_told_no_workbench_and_no_dev_server() {
  let bench = Bench {
    checkout: "shop-on-node".to_owned(),
    tree: Tree::OnNode,
    here: None,
    preamble: Some("Ask before migrating.".to_owned()),
    ..plain()
  };
  let words = bench.words();

  assert!(words.contains("Ask before migrating."), "{words}");
  assert!(!words.contains("/data/workbench"), "{words}");
  assert!(!words.contains("dev-server"), "{words}");
}

// A git checkout of one commit, standing on the branch given.
fn checkout_on(path: &Path, branch: &str) {
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
  git(&["-C", &here, "checkout", "--quiet", "-b", branch]);
}

// The address the checkout's origin remote is set to.
fn with_origin(path: &Path, address: &str) {
  git(&["-C", &at(path), "remote", "add", "origin", address]);
}

// A job of the item working in the checkout `shop`, starting.
async fn job(scratch: &Scratch, id: i64, item: i64) {
  job_titled(scratch, id, item, &format!("item {item}")).await;
}

// A job of the item under its title working in the checkout `shop`, starting.
async fn job_titled(scratch: &Scratch, id: i64, item: i64, title: &str) {
  job_in(scratch, "shop", id, item, title).await;
}

// A job of the item under its title working in the checkout named, starting.
async fn job_in(scratch: &Scratch, checkout: &str, id: i64, item: i64, title: &str) {
  let now = now_ms();

  scratch
    .run(
      r#"INSERT INTO "assignments" ("id", "item_id", "holder_name", "worker_name", "state", "created_at",
           "changed_at")
         VALUES ($1, $2, 'harness', $4, 'running', $3, $3)"#,
      vec![id.into(), item.into(), now.into(), checkout.into()],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "jobs" ("id", "item_id", "lease_id", "holder_name", "assignment_id", "worker_name", "state",
           "run_name", "started_at", "prompt", "item_agent")
         VALUES ($1, $2, 1, 'harness', $1, $6, 'starting', $3, $4, $5, 'worker-01')"#,
      vec![
        id.into(),
        item.into(),
        format!("job-{id}").into(),
        now.into(),
        format!("# {title}\n\nDo the work.").into(),
        checkout.into(),
      ],
    )
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_carries_the_bundle_read_now_under_the_title_and_keeps_it_off_the_job_s_row() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let shop = home.path().join("shop");

  checkout_on(&shop, "work/310");
  repository(
    scratch.db(),
    "shop",
    &at(&shop),
    &[
      ("preamble", "The console stands on port 3000.".into()),
      ("dev_command", "bun run dev".into()),
    ],
  )
  .await;
  job(&scratch, 31, 310).await;

  let sessions = StandIn::answering(&[("Done.", "s-1")]);

  launched(&ground, Some(sessions.clone()), 31, None)
    .await
    .unwrap()
    .expect("the turn's end");

  let turns = sessions.turns();
  let prompt = &turns[0].2;
  let workbench = agency_server::places::dev::workbench(&ground, "shop");

  assert!(prompt.starts_with(&format!("# item 310\n\n{}\n", JOB_PROMPT.bench.heading)), "{prompt}");
  assert!(prompt.contains("`work/310`"), "{prompt}");
  assert!(prompt.contains(&format!("`{}/items/310/`", workbench.display())), "{prompt}");
  assert!(prompt.contains("dev-server restart shop`"), "{prompt}");
  assert!(
    prompt
      .contains(&format!("{}\n\nThe console stands on port 3000.\n", JOB_PROMPT.bench.preamble_heading)),
    "{prompt}"
  );
  assert!(prompt.find("port 3000.").unwrap() < prompt.find("Do the work.").unwrap(), "{prompt}");

  let kept: String = store::one(scratch.db(), r#"SELECT "prompt" FROM "jobs" WHERE "id" = 31"#, vec![])
    .await
    .unwrap()
    .unwrap()
    .try_get("", "prompt")
    .unwrap();

  assert!(!kept.contains(&JOB_PROMPT.bench.heading), "{kept}");
}

// A support worktree's `.git` is a file naming the base's own, and git reads it as a checkout like any other: on
// its branch, with the base's origin, and the address's user and password kept out of the prompt.
#[tokio::test(flavor = "multi_thread")]
async fn a_support_worktree_is_read_as_a_git_checkout_with_its_number_workbench_and_base_preamble() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let shop = home.path().join("shop");
  let worktree = home.path().join("shop-w2");

  checkout_on(&shop, "main-copy");
  with_origin(&shop, "https://tester:secret@example.com/shop.git");
  git(&["-C", &at(&shop), "worktree", "add", "--quiet", "-b", "work/42", &at(&worktree)]);
  assert!(worktree.join(".git").is_file(), "a worktree's .git is a file");
  repository(
    scratch.db(),
    "shop",
    &at(&shop),
    &[("support_count", 1i64.into()), ("preamble", "Ask before migrating.".into())],
  )
  .await;

  let bench = Bench::of_job(&ground, "shop-w2", 42).await.expect("the worktree's bench");

  assert_eq!((bench.checkout.as_str(), bench.repository.as_str()), ("shop-w2", "shop"));
  assert_eq!(bench.support, Some(2));
  assert_eq!(bench.tree, on("work/42"));
  assert_eq!(bench.item_branch(), Some("work/42"));
  assert_eq!(bench.preamble.as_deref(), Some("Ask before migrating."));

  let words = bench.words();

  assert!(
    words.contains(&format!(
      "`{}` is a git checkout on the branch `work/42`; its origin is `https://example.com/shop.git`.",
      at(&worktree)
    )),
    "{words}"
  );
  assert!(!words.contains("secret"), "{words}");

  let here = bench.here.expect("a worktree on this machine");

  assert_eq!(here.workbench, agency_server::places::dev::workbench(&ground, "shop-w2"));
  assert!(here.dev.is_none(), "the repository names no dev command");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_checkout_on_a_work_node_is_read_as_a_git_checkout_with_nothing_of_this_machine() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let shop = home.path().join("shop");

  checkout_on(&shop, "main-copy");
  repository(scratch.db(), "shop", &at(&shop), &[("dev_command", "bun run dev".into())]).await;
  scratch
    .run(
      r#"INSERT INTO "repository_node_checkouts" ("repository", "node", "path", "created_at")
         VALUES ('shop', 'n1', '/remote/shop', 0)"#,
      vec![],
    )
    .await;

  let bench = Bench::of_job(&ground, "shop@n1", 42).await.expect("the node checkout's bench");

  assert_eq!(bench.path, "/remote/shop");
  assert!(bench.tree == Tree::OnNode && bench.support.is_none());
  assert!(bench.here.is_none());
  assert_eq!(bench.words(), "", "nothing is to be told of a plain checkout on a node");
}

// A base checkout cloned from its remote is read with the branch the clone stands on and the remote it came from.
#[tokio::test(flavor = "multi_thread")]
async fn a_plain_clone_is_read_as_a_git_checkout_on_its_branch_with_the_origin_it_was_cloned_from() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let upstream = home.path().join("upstream");
  let shop = home.path().join("shop");

  checkout_on(&upstream, "main-copy");
  git(&["clone", "--quiet", &at(&upstream), &at(&shop)]);
  assert!(shop.join(".git").is_dir(), "a clone's .git is a directory");
  repository(scratch.db(), "shop", &at(&shop), &[("preamble", "Ask before migrating.".into())]).await;

  let bench = Bench::of_job(&ground, "shop", 42).await.expect("the clone's bench");
  let words = bench.words();

  assert_eq!(
    bench.tree,
    Tree::Git {
      head: Head::Branch("main-copy".to_owned()),
      origin: Some(at(&upstream)),
    }
  );
  assert!(
    words.contains(&format!(
      "`{}` is a git checkout on the branch `main-copy`; its origin is `{}`.",
      at(&shop),
      at(&upstream)
    )),
    "{words}"
  );
  assert!(!words.contains("is not a git checkout"), "{words}");
}

// A support checkout whose worktree is not made yet has it made before the prompt's header reads it, so the
// header tells the worktree the job works in, detached at the base's head, never a directory that is none.
#[tokio::test(flavor = "multi_thread")]
async fn a_launch_on_a_support_not_made_yet_tells_the_worktree_it_makes_as_a_git_checkout() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let shop = home.path().join("shop");
  let worktree = home.path().join("shop-w2");

  checkout_on(&shop, "main-copy");
  with_origin(&shop, "https://example.com/shop.git");
  repository(scratch.db(), "shop", &at(&shop), &[("support_count", 1i64.into())]).await;
  job_in(&scratch, "shop-w2", 35, 350, "item 350").await;
  assert!(!worktree.exists(), "the worktree waits for the launch");

  let sessions = StandIn::answering(&[("Done.", "s-5")]);

  launched(&ground, Some(sessions.clone()), 35, None)
    .await
    .unwrap()
    .expect("the turn's end");

  let prompt = &sessions.turns()[0].2;
  let commit = git(&["-C", &at(&shop), "rev-parse", "--short", "HEAD"]);

  assert!(
    prompt.contains(&format!(
      "`{}` is a git checkout detached at commit `{commit}`; its origin is `https://example.com/shop.git`.",
      at(&worktree)
    )),
    "{prompt}"
  );
  assert!(!prompt.contains("is not a git checkout"), "{prompt}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_review_turn_carried_into_a_fresh_session_opens_it_on_the_bundle_while_its_words_carry_none() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let shop = home.path().join("shop");

  checkout_on(&shop, "main-copy");
  repository(
    scratch.db(),
    "shop",
    &at(&shop),
    &[("preamble", "The console stands on port 3000.".into())],
  )
  .await;
  job(&scratch, 32, 320).await;

  let road =
    agency_server::road::for_job(&ground, Some("worker"), Some("shop"), None, &[], Some("harness"))
      .await
      .unwrap();
  let (taken, _) = road.going().expect("a road");
  let left = agency_server::job_review::Resumed {
    provider: taken.provider.clone(),
    model: taken.model.clone(),
    session: "gone-1".to_owned(),
    effort: None,
    prefix: None,
  };
  let sessions = StandIn::answering(&[("Stage two is done.", "fresh-2")]);

  launched(
    &ground,
    Some(sessions.clone()),
    32,
    Some(agency_server::job_review::Turn {
      words: "Do stage two now.".to_owned(),
      resume: Some(left),
    }),
  )
  .await
  .unwrap()
  .expect("the turn's end");

  let started = sessions.started.lock().unwrap().clone();
  let agent_session::Scope::Apart(apart) = &started[0].scope else {
    panic!("an apart session")
  };
  let opening = apart.resume_else.as_deref().expect("the prompt a fresh session opens on");

  assert!(
    opening.starts_with(&format!("# item 320\n\n{}\n", JOB_PROMPT.bench.heading)),
    "{opening}"
  );
  assert!(opening.contains("The console stands on port 3000."), "{opening}");
  assert!(opening.ends_with("Do stage two now."), "{opening}");

  let turns = sessions.turns();

  assert!(!turns[0].2.contains(&JOB_PROMPT.bench.heading), "{}", turns[0].2);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_carries_the_owner_s_preamble_as_the_gate_read_it_into_the_worker_s_language() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let mut ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let shop = home.path().join("shop");
  let greek = "Η κονσόλα βρίσκεται στη θύρα 3000.";
  let translator = std::sync::Arc::new(crate::support::gates::Translator::reading(&[(
    greek,
    "The console stands on port 3000.",
  )]));

  ground.peers.gate = translator.clone();
  checkout_on(&shop, "main-copy");
  repository(scratch.db(), "shop", &at(&shop), &[("preamble", greek.into())]).await;
  job(&scratch, 33, 330).await;

  let sessions = StandIn::answering(&[("Done.", "s-3")]);

  launched(&ground, Some(sessions.clone()), 33, None)
    .await
    .unwrap()
    .expect("the turn's end");

  let prompt = &sessions.turns()[0].2;

  assert!(
    prompt
      .contains(&format!("{}\n\nThe console stands on port 3000.\n", JOB_PROMPT.bench.preamble_heading)),
    "{prompt}"
  );
  assert!(!prompt.contains(greek), "{prompt}");
  assert_eq!(translator.calls().len(), 1, "the preamble alone was asked");

  let kept: String =
    store::one(scratch.db(), r#"SELECT "preamble" FROM "repositories" WHERE "name" = 'shop'"#, vec![])
      .await
      .unwrap()
      .unwrap()
      .try_get("", "preamble")
      .unwrap();

  assert_eq!(kept, greek, "the row keeps the owner's text as written");
}

// The prompt the first turn of a job of an item filed under `title` opens on, launched with `gate`.
async fn opening(title: &str, gate: std::sync::Arc<dyn agency_server::peers::gate::Gate>) -> String {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let (_peers, vars) = crate::support::peers::stand().await;
  let mut ground = seeded_with(scratch.db(), home.path(), &vars).await;
  let shop = home.path().join("shop");

  ground.peers.gate = gate;
  checkout_on(&shop, "main-copy");
  repository(scratch.db(), "shop", &at(&shop), &[]).await;
  job_titled(&scratch, 34, 340, title).await;

  let sessions = StandIn::answering(&[("Done.", "s-4")]);

  launched(&ground, Some(sessions.clone()), 34, None)
    .await
    .unwrap()
    .expect("the turn's end");

  sessions.turns()[0].2.clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_opens_on_the_item_s_title_as_the_gate_read_it_into_the_worker_s_language() {
  let greek = "Η ουρά δείχνει τους τίτλους";
  let translator = crate::support::gates::Translator::reading(&[(greek, "The queue shows the titles")]);
  let prompt = opening(greek, std::sync::Arc::new(translator)).await;

  assert!(prompt.starts_with("# The queue shows the titles\n"), "{prompt}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_launch_with_no_reading_of_its_title_opens_on_the_title_as_written_with_the_mark_after_it() {
  let greek = "Κανένας τίτλος";
  let prompt = opening(greek, crate::support::gates::none()).await;

  assert!(
    prompt.starts_with(&format!("# {greek} {}\n", JOB_PROMPT.translation.untranslated_title)),
    "{prompt}"
  );
}

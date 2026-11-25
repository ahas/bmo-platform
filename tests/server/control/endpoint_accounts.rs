//! An endpoint account (docs/cache-fallback.md, 「The Claude Code road is a list of accounts」): added and
//! read back as its kind, open while it is on, its key file is there and not empty and no rest stands on
//! it, its mark lifted by its key written again or by the owner changing it, never asked of the window
//! reader, and passed over by a turn that does not take it — a warm ping, or a run on a work node — with
//! nothing written under it.

use std::collections::HashMap;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::claude_account_changes::{BasePromptGiven, CredentialGiven, add, update};
use agency_server::control::claude_accounts::{
  Account, Credential, CredentialKind, EndpointModels, Judging, accounts, open_accounts, walk_for_user,
};
use agency_server::job_run::claude::failure::{AccountFailure, Failure, of_errored};
use agency_server::job_run::claude::renewal::Renewing;
use agency_server::job_run::claude::walk::{Attempt, Walking, on_accounts};
use agency_server::now_ms;
use agency_server::places::Ground;
use agency_server::usage_windows::window_logins;
use cache_siren::Keeping;
use harness_providers::Machine;

use crate::support::grounds::signed_in;
use crate::support::scratch::Scratch;

// The model the walks' runs go out on.
const MODEL: &str = "claude-opus-5-5";

// A directory account `main`, signed in, and an endpoint account `deep`, each with a home of its own, and the
// endpoint's key written into its file.
struct Logins {
  scratch: Scratch,
  dir: tempfile::TempDir,
}

impl Logins {
  async fn new() -> Self {
    let scratch = Scratch::new().await;
    let dir = tempfile::tempdir().unwrap();
    let main = dir.path().join("main");
    let deep = dir.path().join("deep");

    signed_in(&main);
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::write(dir.path().join("deep.key"), "sk-endpoint\n").unwrap();

    let logins = Self { scratch, dir };
    let directory = CredentialGiven {
      path: Some(main.to_string_lossy().into_owned()),
      ..CredentialGiven::default()
    };
    let endpoint = CredentialGiven {
      kind: Some(CredentialKind::Endpoint),
      path: Some(deep.to_string_lossy().into_owned()),
      endpoint: Some("https://endpoint.test/anthropic".into()),
      key_file: Some(logins.key().to_string_lossy().into_owned()),
      models: Some(EndpointModels {
        opus: Some("big-model".into()),
        sonnet: Some("small-model".into()),
        haiku: Some("small-model".into()),
      }),
    };

    add(logins.scratch.db(), "main", &directory, BasePromptGiven::Keep).await.unwrap();
    add(logins.scratch.db(), "deep", &endpoint, BasePromptGiven::Keep).await.unwrap();

    logins
  }

  fn key(&self) -> std::path::PathBuf {
    self.dir.path().join("deep.key")
  }

  async fn open(&self, windows: &Keeping) -> Vec<String> {
    let judging = Judging {
      db: self.scratch.db(),
      windows,
      data_root: std::path::Path::new("/nonexistent"),
    };

    open_accounts(judging).await.unwrap().into_iter().map(|account| account.id).collect()
  }

  async fn deep(&self) -> Account {
    accounts(self.scratch.db())
      .await
      .unwrap()
      .into_iter()
      .find(|one| one.id == "deep")
      .unwrap()
  }
}

#[tokio::test]
async fn an_endpoint_account_is_read_back_as_its_kind_with_its_url_key_file_and_models() {
  let logins = Logins::new().await;
  let deep = logins.deep().await;

  assert_eq!(
    deep.credential,
    Credential::Endpoint {
      home: logins.dir.path().join("deep"),
      url: "https://endpoint.test/anthropic".into(),
      key_file: logins.key(),
      models: EndpointModels {
        opus: Some("big-model".into()),
        sonnet: Some("small-model".into()),
        haiku: Some("small-model".into()),
      },
    }
  );

  // The key is nowhere in the row.
  let row = logins
    .scratch
    .count(
      r#"SELECT COUNT(*) FROM "claude_accounts" WHERE "claude_accounts"::text LIKE '%sk-endpoint%'"#,
      vec![],
    )
    .await;

  assert_eq!(row, 0);

  // A row of a kind no member answers for is left out rather than read as another kind.
  logins
    .scratch
    .run(
      r#"INSERT INTO "claude_accounts" ("id", "name", "place", "credential_kind", "credential_path", "added_at")
         VALUES ('stray', 'stray', 9, 'cookie', '/nowhere', 1)"#,
      vec![],
    )
    .await;

  let held: Vec<String> = accounts(logins.scratch.db())
    .await
    .unwrap()
    .into_iter()
    .map(|account| account.id)
    .collect();

  assert_eq!(held, ["main", "deep"]);
}

#[tokio::test]
async fn an_endpoint_account_is_open_while_its_key_file_holds_a_key_and_no_rest_stands_on_it() {
  let logins = Logins::new().await;
  let windows = Keeping::new(logins.scratch.db().clone(), vec![]);

  assert_eq!(logins.open(&windows).await, ["main", "deep"]);

  std::fs::write(logins.key(), "").unwrap();
  assert_eq!(logins.open(&windows).await, ["main"], "an empty key file closes it");

  std::fs::remove_file(logins.key()).unwrap();
  assert_eq!(logins.open(&windows).await, ["main"], "a missing key file closes it");

  std::fs::write(logins.key(), "sk-endpoint\n").unwrap();
  logins
    .scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_until:deep', $1)"#,
      vec![(now_ms() / 1000 + 600).to_string().into()],
    )
    .await;
  assert_eq!(logins.open(&windows).await, ["main"], "a rest closes it");
}

#[tokio::test]
async fn an_endpoint_accounts_mark_goes_with_its_key_written_after_it_or_with_the_owners_change() {
  let logins = Logins::new().await;
  let windows = Keeping::new(logins.scratch.db().clone(), vec![]);
  let mark = |at: i64| {
    logins.scratch.run(
      r#"UPDATE "claude_accounts" SET "needs_login_at" = $1 WHERE "id" = 'deep'"#,
      vec![at.into()],
    )
  };
  let now = now_ms() / 1000;

  // Marked after its key was written: it waits for a person.
  mark(now + 60).await;
  assert_eq!(logins.open(&windows).await, ["main"]);
  assert!(logins.deep().await.needs_login_at.is_some());

  // Its key written after the mark: it stands again, and the mark goes.
  mark(now - 60).await;
  assert_eq!(logins.open(&windows).await, ["main", "deep"]);
  assert_eq!(logins.deep().await.needs_login_at, None);

  // The owner changing the account lifts the mark, since a balance topped up leaves nothing to read.
  mark(now + 60).await;
  update(logins.scratch.db(), "deep", Some("deep two"), None, None, BasePromptGiven::Keep)
    .await
    .unwrap();
  assert_eq!(logins.deep().await.needs_login_at, None);
  assert_eq!(logins.open(&windows).await, ["main", "deep"]);
}

#[tokio::test]
async fn an_endpoint_account_is_never_asked_of_the_window_reader() {
  let logins = Logins::new().await;
  let asked: Vec<String> = window_logins(accounts(logins.scratch.db()).await.unwrap())
    .into_iter()
    .map(|login| login.account)
    .collect();

  assert_eq!(asked, ["main"]);
}

// A turn that takes only the logins a warm ping or a run on a work node may go out on, keeping what it
// tried.
struct Turn {
  tried: Vec<String>,
}

impl Renewing for Turn {}

impl Attempt<String> for Turn {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let id = account.map(|account| account.id).unwrap_or_default();

    self.tried.push(id.clone());
    Ok(id)
  }

  fn takes(&self, account: &Account) -> bool {
    account.credential.warmed() && account.credential.runs_on_nodes()
  }
}

// A turn that meets the endpoint's refusal on the endpoint account and answers on any other.
struct Refusing {
  failure: Failure,
  tried: Vec<String>,
}

impl Renewing for Refusing {}

impl Attempt<String> for Refusing {
  async fn attempt(&mut self, account: Option<Account>) -> Result<String, Failure> {
    let id = account.map(|account| account.id).unwrap_or_default();

    self.tried.push(id.clone());

    if id == "deep" { Err(self.failure.clone()) } else { Ok(id) }
  }
}

#[tokio::test]
async fn an_endpoints_refusal_marks_or_rests_its_account_and_the_walk_moves_on() {
  let refused_401 = "Please run /login · API Error: 401 Authentication Fails, Your api key: ****0000 is invalid (request_id: cf3dd8cd-e802-4102-bd4d-e573aa73c839)";
  let refused_402 = "API Error: 402 Insufficient Balance";
  let refused_429 = "API Error: Request rejected (429) · this may be a temporary capacity issue.";

  for (said, rested) in [(refused_401, false), (refused_402, false), (refused_429, true)] {
    let logins = Logins::new().await;

    // Its key was written long before any mark this run sets, so the mark stands rather than being read
    // as a key written after it.
    std::fs::File::options()
      .write(true)
      .open(logins.key())
      .unwrap()
      .set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000))
      .unwrap();

    let home = tempfile::tempdir().unwrap();
    let machine = Machine::with_env(
      [("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
      home.path().join("data"),
    );
    let variables: HashMap<String, String> = HashMap::new();
    let service =
      Service::new(logins.scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
    let ground = Ground::new(&service, &machine, None);
    let walking = Walking {
      ground: &ground,
      model: MODEL,
    };

    // The endpoint first in the walk, so the refusal is met before the account that answers.
    logins
      .scratch
      .run(r#"UPDATE "claude_accounts" SET "place" = 0 WHERE "id" = 'deep'"#, vec![])
      .await;

    let mut turn = Refusing {
      failure: of_errored(said, "bmo"),
      tried: vec![],
    };

    assert_eq!(on_accounts(&walking, "bmo", None, &mut turn).await, Ok("main".to_owned()), "{said}");
    assert_eq!(turn.tried, ["deep", "main"], "{said}");

    let deep = logins.deep().await;
    let count = |key: String| {
      let scratch = &logins.scratch;

      async move {
        scratch
          .count(r#"SELECT COUNT(*) FROM "settings" WHERE "key" = $1"#, vec![key.into()])
          .await
      }
    };
    let rest = count("rest_until:deep".to_owned()).await;
    let model_rest = count(format!("model_rest:deep:{MODEL}")).await;
    let walk = walk_for_user(Judging::of(&ground), None, Some(MODEL)).await.unwrap();

    // Either way the account is closed to the next run on the model.
    assert_eq!(walk.into_iter().map(|account| account.id).collect::<Vec<_>>(), ["main"], "{said}");
    assert_eq!(rest, 0, "{said}: no rest of the account's own");

    if rested {
      assert_eq!(model_rest, 1, "{said}: the run's model rests on the endpoint until a reading decides");
      assert_eq!(deep.needs_login_at, None, "{said}: a short limit is no login gone");
    } else {
      assert_eq!(model_rest, 0, "{said}: a refused key or balance is no usage rest");
      assert!(deep.needs_login_at.is_some(), "{said}: the account waits for a person");
      assert_eq!(logins.open(&Keeping::new(logins.scratch.db().clone(), vec![])).await, ["main"]);
    }
  }
}

#[tokio::test]
async fn a_turn_that_does_not_take_an_endpoint_account_passes_it_over_and_writes_nothing_under_it() {
  let logins = Logins::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = Machine::with_env(
    [("HOME".to_owned(), home.path().to_string_lossy().into_owned())],
    home.path().join("data"),
  );
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(logins.scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let ground = Ground::new(&service, &machine, None);
  let walking = Walking {
    ground: &ground,
    model: MODEL,
  };

  // The endpoint first in the walk: the turn goes out on the directory account alone.
  logins
    .scratch
    .run(r#"UPDATE "claude_accounts" SET "place" = 0 WHERE "id" = 'deep'"#, vec![])
    .await;

  let mut turn = Turn { tried: vec![] };

  assert_eq!(on_accounts(&walking, "bmo", None, &mut turn).await, Ok("main".to_owned()));
  assert_eq!(turn.tried, ["main"]);

  // With the endpoint the only login on, the turn goes out on none, and nothing is marked or rested.
  logins
    .scratch
    .run(r#"UPDATE "claude_accounts" SET "enabled" = 0 WHERE "id" = 'main'"#, vec![])
    .await;

  let mut turn = Turn { tried: vec![] };
  let walked = on_accounts(&walking, "bmo", None, &mut turn).await;

  assert!(matches!(
    walked,
    Err(Failure::Account {
      failure: AccountFailure::Login,
      ..
    })
  ));
  assert!(turn.tried.is_empty());
  assert_eq!(logins.deep().await.needs_login_at, None);
  assert_eq!(
    logins
      .scratch
      .count(r#"SELECT COUNT(*) FROM "settings" WHERE "key" LIKE 'rest_until%'"#, vec![])
      .await,
    0
  );
}

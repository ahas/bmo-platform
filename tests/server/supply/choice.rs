//! The source a run takes as it starts (docs/supply.md, 「Choosing a source as a run starts」, 「Owned
//! supply」): the deployment's own runs and the administrator's the platform's, unreserved and unasked; a
//! person's personal supply alone with no hook; a member or registration the person owns their personal
//! supply, its refusal marking no key of theirs and passing it over to no platform source, and another
//! person's no platform source; with a hook its order walked past a refused reservation to the first
//! accepted, the run closed with the policy's sentence where every source refuses, and personal supply alone
//! where the hook fails or answers what does not read; and a Claude account the administrator marked lendable
//! lent on a platform source whoever owns it, never to its owner (「The lendable accounts」).

use agency_server::control::claude_account_changes::set_lendable;
use agency_server::control::claude_accounts::{Carrying, carried_for_user, moment_words};
use agency_server::supply::reservations::held;
use agency_server::supply::{
  Choice, Chosen, Shortfall, Source, carrying, choose, pass_owned, refuse_personal_key,
};
use axum::http::StatusCode;
use policy::fill_marks;
use policy::supply::SUPPLY;
use serde_json::json;

use super::{Consumer, PERSON, asked, personal_key, row_of, state_of};
use crate::support::grounds::{deepseek_road, ground_on, signed_in};
use crate::support::scratch::Scratch;

fn going(choice: Choice) -> Chosen {
  match choice {
    Choice::Go(chosen) => chosen,
    Choice::Closed(closed) => panic!("the run was due to go out: {closed:?}"),
  }
}

// The source a closed run named last, and why it could not be taken.
fn closed(choice: Choice) -> (String, String) {
  match choice {
    Choice::Closed(closed) => (closed.source, closed.reason),
    Choice::Go(chosen) => panic!("the run was due to be closed: {chosen:?}"),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn the_deployment_s_own_runs_and_the_administrator_s_go_out_unreserved_and_unasked() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;

  consumer.register(&ground);
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '1')"#, vec![])
    .await;

  for account in [None, Some(1)] {
    let chosen = going(choose(&ground, &asked(account, "deepseek")).await.unwrap());

    assert_eq!((chosen.source, chosen.account), (Source::Deployment, None), "account {account:?}");
  }

  assert!(consumer.heard().is_empty(), "the hook is asked nothing for the deployment's own runs");
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn with_no_hook_a_person_takes_their_own_supply_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());

  // A Claude Code run of a person with no login of their own is lent no platform account either.
  for provider in ["deepseek", "claude-code"] {
    assert_eq!(
      closed(choose(&ground, &asked(Some(PERSON), provider)).await.unwrap()),
      ("personal".to_owned(), fill_marks(&SUPPLY.words.no_personal, &[("provider", provider)])),
      "{provider}"
    );
  }

  personal_key(scratch.db(), "deepseek").await;

  let chosen = going(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap());

  assert_eq!((chosen.source, chosen.order), (Source::Personal, vec!["personal".to_owned()]));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_s_own_member_or_registration_is_their_personal_supply_and_another_s_offers_no_platform() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;

  deepseek_road(home.path());
  registration_named(&scratch, "deepseek-flash").await;
  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["personal", "plan"] }));
  consumer.answers("reserve:plan", json!({ "accepted": true, "reservation": "r-plan", "amountUsd": 1.0 }));

  for (owner, kind) in [(PERSON, "registered"), (8, "registered"), (PERSON, "deepseek"), (8, "deepseek")] {
    owned_by(&scratch, home.path(), owner).await;

    let choice = choose(&ground, &asked(Some(PERSON), kind)).await.unwrap();

    if owner == PERSON {
      assert_eq!(going(choice).source, Source::Owned, "{kind} owned by the person");
    } else {
      let owned = fill_marks(&SUPPLY.words.owned, &[("model", "deepseek-flash")]);

      assert_eq!(closed(choice), ("plan".to_owned(), owned), "{kind} owned by another");
    }
  }

  assert!(
    !consumer.ops().contains(&"reserve".to_owned()),
    "nothing is reserved on a person's own model"
  );
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "supply_reservations""#, vec![]).await, 0);
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refusal_of_a_model_the_person_owns_marks_no_key_of_theirs() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let refused =
    || scratch.count(r#"SELECT COUNT(*) FROM "personal_supply" WHERE "refused_at" IS NOT NULL"#, vec![]);

  deepseek_road(home.path());
  registration_named(&scratch, "deepseek-flash").await;
  personal_key(scratch.db(), "deepseek").await;

  // The member the person owns is taken before their key, and its refusal leaves the key standing.
  owned_by(&scratch, home.path(), PERSON).await;

  let owned = going(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap());

  assert_eq!(owned.source, Source::Owned);
  assert_eq!(refuse_personal_key(&ground, &owned).await.unwrap(), None);
  assert_eq!(refused().await, 0);

  // On another's member the run goes out on the person's key, which its refusal marks.
  owned_by(&scratch, home.path(), 8).await;

  let keyed = going(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap());

  assert_eq!(keyed.source, Source::Personal);
  assert!(refuse_personal_key(&ground, &keyed).await.unwrap().is_some());
  assert_eq!(refused().await, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_model_the_person_owns_passed_over_offers_them_no_platform_source() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;

  deepseek_road(home.path());
  registration_named(&scratch, "deepseek-flash").await;
  owned_by(&scratch, home.path(), PERSON).await;
  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["personal", "plan"] }));
  consumer.answers("reserve:plan", json!({ "accepted": true, "reservation": "r-plan", "amountUsd": 1.0 }));

  let owned = going(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap());
  let passed = pass_owned(&ground, &owned).await.unwrap();

  assert_eq!(
    closed(passed),
    ("plan".to_owned(), fill_marks(&SUPPLY.words.owned, &[("model", "deepseek-flash")]))
  );
  assert!(
    !consumer.ops().contains(&"reserve".to_owned()),
    "nothing is reserved past a person's own model"
  );
  consumer.down().await;
}

// A registration under the name given, with no address.
async fn registration_named(scratch: &Scratch, name: &str) {
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
           "thinking", "walked", "added_at") VALUES ($1, $1, $1, 32768, 4096, 0, 0, 0)"#,
      vec![name.into()],
    )
    .await;
}

// The deepseek road's member and the registration of its name both owned by the account given.
async fn owned_by(scratch: &Scratch, home: &std::path::Path, account: i64) {
  let file = home.join("data/llm-gateway/members.json");
  let mut members: serde_json::Value =
    serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();

  members[0]["account"] = account.into();
  std::fs::write(&file, members.to_string()).unwrap();
  scratch
    .run(r#"UPDATE "registered_models" SET "owner_account" = $1"#, vec![account.into()])
    .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hook_s_order_is_walked_past_a_refused_reservation_to_the_first_accepted() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let expires = agency_server::now_ms() + 3_600_000;

  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["personal", "credits", "plan"] }));
  consumer.answers("reserve:credits", json!({ "accepted": false }));
  consumer.answers(
    "reserve:plan",
    json!({ "accepted": true, "reservation": "r-plan", "amountUsd": 1.25, "expiresAt": expires }),
  );

  let chosen = going(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap());
  let row = row_of(&chosen);
  let kept = held(scratch.db(), row).await.unwrap().unwrap();
  let (state, _) = state_of(scratch.db(), row).await;

  assert_eq!(chosen.source.word(), "plan");
  assert_eq!(chosen.order, ["personal", "credits", "plan"]);
  assert_eq!(
    (kept.run.as_str(), kept.account, kept.source.as_str(), kept.reservation.as_str()),
    (chosen.run.as_str(), PERSON, "plan", "r-plan")
  );
  assert_eq!(
    (kept.amount_micros, kept.unit_micros, kept.expires_at, state.as_str()),
    (1_250_000, 1_250_000, Some(expires), "open")
  );

  let heard = consumer.heard();
  let order = &heard[0].ask;

  assert_eq!(consumer.ops(), ["order", "reserve", "reserve"]);
  assert_eq!(
    (&order["account"], &order["provider"], &order["kind"], &order["job"]),
    (&json!(PERSON), &json!("deepseek"), &json!("job"), &json!(41))
  );
  assert_eq!(
    consumer.keys(),
    [format!("{}:1:reserve", chosen.run), format!("{}:2:reserve", chosen.run)]
  );
  assert_eq!(heard[1].ask["source"], "credits");
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn every_source_refused_closes_the_run_with_the_policy_s_sentence() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let reopens = agency_server::now_ms() + 7_200_000;

  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["credits", "plan"] }));
  consumer.answers("reserve:credits", json!({ "accepted": false }));
  consumer.answers("reserve:plan", json!({ "accepted": false, "reopensAt": reopens }));

  let Choice::Closed(closed) = choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap() else {
    panic!("the run was due to be closed");
  };
  let reopening = fill_marks(&SUPPLY.words.reopens, &[("at", &moment_words(reopens / 1000))]);

  assert_eq!((closed.source.as_str(), closed.reopens_at), ("plan", Some(reopens)));
  assert_eq!(closed.why, Shortfall::Refused);
  assert_eq!(closed.reason, fill_marks(&SUPPLY.words.refused, &[("source", "plan")]));
  assert_eq!(
    closed.sentence,
    fill_marks(
      &SUPPLY.words.closed.refused,
      &[
        ("source", "plan"),
        ("reopens", &reopening),
        ("settings", &SUPPLY.words.settings_section)
      ]
    )
  );
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "supply_reservations""#, vec![]).await, 0);
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_hook_that_fails_or_does_not_read_leaves_personal_supply_alone() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let no_personal = fill_marks(&SUPPLY.words.no_personal, &[("provider", "deepseek")]);

  consumer.register(&ground);
  consumer.answers("reserve", json!({ "accepted": true, "reservation": "r", "amountUsd": 1.0 }));
  consumer.replies("order", StatusCode::INTERNAL_SERVER_ERROR, json!({}));

  assert_eq!(
    closed(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap()),
    ("personal".to_owned(), no_personal.clone())
  );

  consumer.replies("order", StatusCode::OK, json!({ "answer": "not an answer" }));

  assert_eq!(
    closed(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap()),
    ("personal".to_owned(), no_personal)
  );

  personal_key(scratch.db(), "deepseek").await;

  let chosen = going(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap());

  assert_eq!((chosen.source, chosen.order), (Source::Personal, vec!["personal".to_owned()]));
  assert_eq!(consumer.ops(), ["order", "order", "order"], "nothing is reserved");
  assert_eq!(scratch.count(r#"SELECT COUNT(*) FROM "supply_reservations""#, vec![]).await, 0);
  consumer.down().await;
}

// The ids of the Claude accounts a run carrying `carrying` may go out on, in order.
async fn carried(scratch: &Scratch, carrying: Carrying) -> Vec<String> {
  carried_for_user(scratch.db(), carrying)
    .await
    .unwrap()
    .into_iter()
    .map(|account| account.id)
    .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn an_account_marked_lendable_is_lent_on_a_platform_source_whoever_owns_it_and_never_to_its_owner() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let login = home.path().join("login");

  signed_in(&login);
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "owner_account")
         VALUES ('owner', 'owner', 1, 1, 'directory', $1, 0, 1),
                ('eight', 'eight', 2, 1, 'directory', $1, 0, 8)"#,
      vec![login.to_string_lossy().into_owned().into()],
    )
    .await;
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '1')"#, vec![])
    .await;
  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["personal", "global"] }));
  consumer.answers(
    "reserve:global",
    json!({ "accepted": true, "reservation": "r-global", "amountUsd": 1.0 }),
  );

  // Unmarked, the administrator's account is lent to nobody.
  assert_eq!(
    closed(choose(&ground, &asked(Some(PERSON), "claude-code")).await.unwrap()),
    (
      "global".to_owned(),
      fill_marks(&SUPPLY.words.no_lendable, &[("provider", "claude-code")])
    )
  );

  set_lendable(scratch.db(), "owner", true).await.unwrap();

  let chosen = going(choose(&ground, &asked(Some(PERSON), "claude-code")).await.unwrap());

  assert_eq!(chosen.source.word(), "global");
  assert_eq!(carrying(Some(&chosen)), Carrying::Lent(PERSON));
  assert_eq!(carried(&scratch, Carrying::Lent(PERSON)).await, ["owner"]);

  // The owner's own runs still carry it as their own, and a person's lendable account is lent to others alone.
  set_lendable(scratch.db(), "eight", true).await.unwrap();

  assert_eq!(carried(&scratch, Carrying::Platform).await, ["owner"]);
  assert_eq!(carried(&scratch, Carrying::Own(8)).await, ["eight"]);
  assert_eq!(carried(&scratch, Carrying::Lent(8)).await, ["owner"]);
  assert_eq!(carried(&scratch, Carrying::Lent(PERSON)).await, ["owner", "eight"]);
  consumer.down().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_closed_run_names_the_reason_the_last_source_failed_in_a_sentence_of_its_own() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let why = |choice: Choice| match choice {
    Choice::Closed(closed) => (closed.why, closed.sentence),
    Choice::Go(chosen) => panic!("the run was due to be closed: {chosen:?}"),
  };

  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["personal", "global"] }));
  consumer.answers("reserve:global", json!({ "accepted": true, "reservation": "r", "amountUsd": 1.0 }));

  // A Claude Code run of a person with no login of their own, and no platform account lent: the platform's credits
  // are not what ran out, so the person is not told to top up.
  let (shortfall, sentence) = why(choose(&ground, &asked(Some(PERSON), "claude-code")).await.unwrap());

  assert_eq!(shortfall, Shortfall::NoLendable);
  assert_eq!(shortfall.word(), "no-lendable");
  assert_eq!(
    sentence,
    fill_marks(&SUPPLY.words.closed.no_lendable, &[("settings", &SUPPLY.words.settings_section)])
  );
  assert!(!sentence.contains("Top up"), "{sentence}");

  // A gateway road whose only key of the person's own its provider refused, the platform's source refusing too: the
  // last tried names why.
  consumer.answers("order", json!({ "sources": ["global", "personal"] }));
  consumer.answers("reserve:global", json!({ "accepted": false }));
  personal_key(scratch.db(), "deepseek").await;
  scratch
    .run(
      r#"UPDATE "personal_supply" SET "refused_at" = 1 WHERE "owner_account" = $1"#,
      vec![PERSON.into()],
    )
    .await;

  let (shortfall, sentence) = why(choose(&ground, &asked(Some(PERSON), "deepseek")).await.unwrap());

  assert_eq!(shortfall, Shortfall::OwnRefused);
  assert_eq!(
    sentence,
    fill_marks(&SUPPLY.words.closed.own_refused, &[("settings", &SUPPLY.words.settings_section)])
  );

  // Every reason says its own sentence.
  let sentences = [
    &SUPPLY.words.closed.no_order,
    &SUPPLY.words.closed.withheld,
    &SUPPLY.words.closed.no_personal,
    &SUPPLY.words.closed.own_refused,
    &SUPPLY.words.closed.no_organisation,
    &SUPPLY.words.closed.organisation_refused,
    &SUPPLY.words.closed.no_lendable,
    &SUPPLY.words.closed.shared_node,
    &SUPPLY.words.closed.owned,
    &SUPPLY.words.closed.refused,
    &SUPPLY.words.closed.ran_out,
  ];

  assert_eq!(sentences.iter().collect::<std::collections::BTreeSet<_>>().len(), sentences.len());
  consumer.down().await;
}

// A login lent to a person whose PC other accounts share would stand where their programs read it, so none is lent
// there, and the run is told why; on a PC of their own the same account is lent again.
#[tokio::test(flavor = "multi_thread")]
async fn no_login_is_lent_to_a_person_whose_node_other_accounts_share() {
  use agency_server::person_nodes;

  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let ground = ground_on(scratch.db().clone(), home.path());
  let consumer = Consumer::stand().await;
  let login = home.path().join("login");
  let person = PERSON.to_string();

  signed_in(&login);
  scratch.run(r#"DELETE FROM "claude_accounts""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "owner_account")
         VALUES ('owner', 'owner', 1, 1, 'directory', $1, 0, 1)"#,
      vec![login.to_string_lossy().into_owned().into()],
    )
    .await;
  scratch
    .run(r#"INSERT INTO "settings" ("key", "value") VALUES ('users_administrator', '1')"#, vec![])
    .await;
  set_lendable(scratch.db(), "owner", true).await.unwrap();
  consumer.register(&ground);
  consumer.answers("order", json!({ "sources": ["global"] }));
  consumer.answers(
    "reserve:global",
    json!({ "accepted": true, "reservation": "r-global", "amountUsd": 1.0 }),
  );

  // Declared shared but alone on it, the person is lent the login.
  person_nodes::set(scratch.db(), &person, "pc-shared", true, false)
    .await
    .unwrap()
    .unwrap();
  assert_eq!(person_nodes::shared_node(scratch.db(), &person).await.unwrap(), None);
  assert_eq!(carried(&scratch, Carrying::Lent(PERSON)).await, ["owner"]);

  // Another account on the same PC, and nothing is lent to either.
  person_nodes::set(scratch.db(), "8", "pc-shared", true, false).await.unwrap().unwrap();
  assert_eq!(
    person_nodes::shared_node(scratch.db(), &person).await.unwrap().as_deref(),
    Some("pc-shared")
  );
  assert!(carried(&scratch, Carrying::Lent(PERSON)).await.is_empty());
  assert!(carried(&scratch, Carrying::Lent(8)).await.is_empty());
  // The owner's own runs, and the platform's, still carry it.
  assert_eq!(carried(&scratch, Carrying::Platform).await, ["owner"]);

  match choose(&ground, &asked(Some(PERSON), "claude-code")).await.unwrap() {
    Choice::Closed(closed) => {
      assert_eq!(closed.why, Shortfall::SharedNode);
      assert_eq!(closed.why.word(), "shared-node");
      assert_eq!(closed.reason, fill_marks(&SUPPLY.words.shared_node, &[("provider", "claude-code")]));
      assert_eq!(
        closed.sentence,
        fill_marks(&SUPPLY.words.closed.shared_node, &[("settings", &SUPPLY.words.settings_section)])
      );
    }
    Choice::Go(chosen) => panic!("no lent login goes to a shared PC: {chosen:?}"),
  }

  // The other account moved to a PC of its own, the person's PC is theirs alone again.
  person_nodes::set(scratch.db(), "8", "pc-eight", false, false).await.unwrap().unwrap();
  person_nodes::clear_one(scratch.db(), "8", "pc-shared").await.unwrap();

  let chosen = going(choose(&ground, &asked(Some(PERSON), "claude-code")).await.unwrap());

  assert_eq!(carrying(Some(&chosen)), Carrying::Lent(PERSON));
  assert_eq!(carried(&scratch, Carrying::Lent(PERSON)).await, ["owner"]);
  consumer.down().await;
}

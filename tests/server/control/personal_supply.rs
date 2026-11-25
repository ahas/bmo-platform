//! A person's own keys for the gateway's roads (docs/supply.md, 「What a person brings」; docs/api.md, 「accounts,
//! rest and usage」), through the agency's own client: added last of the person's keys of a road some member
//! serves, moved, turned back on out of a refusal and removed by that person alone, read by the platform as the
//! roads they serve; and a person off-boarded, each account and key of theirs turned off and their call key
//! revoked.

use agency_server::call_keys;
use bmo_agency_client::{PersonalSupplyAsk, PersonalSupplyChange};

use super::owned_accounts::{client, front, refused};
use crate::support::grounds::{deepseek_road, ground_on};
use crate::support::scratch::Scratch;

fn ask(road: &str, secret: &str) -> PersonalSupplyAsk {
  PersonalSupplyAsk {
    road: road.to_owned(),
    secret: secret.to_owned(),
  }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_person_keeps_their_own_keys_of_a_served_road_and_another_finds_none_of_them() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();

  deepseek_road(home.path());

  let port = front(ground_on(scratch.db().clone(), home.path())).await;
  let seven = client(port, Some(("7", "member")));
  let eight = client(port, Some(("8", "member")));
  let platform = client(port, None);

  let first = seven.create_personal_supply(&ask("deepseek", "ds")).await.unwrap();
  let second = seven.create_personal_supply(&ask("deepseek", "ds-spare")).await.unwrap();

  assert_eq!((first.place, second.place, first.enabled), (1, 2, true));
  assert_eq!(
    refused(seven.create_personal_supply(&ask("codex", "cx")).await),
    (422, "unknown-road".to_owned())
  );
  assert_eq!(
    refused(seven.create_personal_supply(&ask("deepseek", "../ds")).await),
    (400, "bad-request".to_owned())
  );
  assert_eq!(
    refused(platform.create_personal_supply(&ask("deepseek", "ds")).await),
    (403, "forbidden".to_owned())
  );

  // Moved ahead, the spare stands first and the first behind it.
  let moved = seven
    .change_personal_supply(
      second.id,
      &PersonalSupplyChange {
        place: Some(1),
        enabled: None,
      },
    )
    .await
    .unwrap();
  let places = |page: bmo_agency_client::PersonalSupplyPage| -> Vec<(String, i64)> {
    page.items.unwrap().into_iter().map(|key| (key.secret, key.place)).collect()
  };

  assert_eq!(moved.place, 1);
  assert_eq!(
    places(seven.list_personal_supply().await.unwrap()),
    [("ds-spare".to_owned(), 1), ("ds".to_owned(), 2)]
  );

  // A refused key turned on is no longer refused.
  scratch
    .run(
      r#"UPDATE "personal_supply" SET "refused_at" = 5 WHERE "id" = $1"#,
      vec![first.id.into()],
    )
    .await;

  let reopened = seven
    .change_personal_supply(
      first.id,
      &PersonalSupplyChange {
        place: None,
        enabled: Some(true),
      },
    )
    .await
    .unwrap();

  assert_eq!(reopened.refused_at, None);

  // Another person reads none of them, and finds none to change or remove.
  assert_eq!(eight.list_personal_supply().await.unwrap().items.map(|items| items.len()), Some(0));
  assert_eq!(
    refused(
      eight
        .change_personal_supply(
          first.id,
          &PersonalSupplyChange {
            place: None,
            enabled: Some(false),
          }
        )
        .await
    ),
    (404, "unknown-supply".to_owned())
  );
  assert_eq!(
    refused(eight.delete_personal_supply(first.id).await),
    (404, "unknown-supply".to_owned())
  );

  // The platform reads the roads alone.
  let people = platform.list_personal_supply().await.unwrap();

  assert_eq!(people.items, None);
  assert_eq!(
    people
      .people
      .unwrap()
      .into_iter()
      .map(|person| (person.user, person.roads))
      .collect::<Vec<_>>(),
    [(7, vec!["deepseek".to_owned()])]
  );

  seven.delete_personal_supply(first.id).await.unwrap();

  assert_eq!(places(seven.list_personal_supply().await.unwrap()), [("ds-spare".to_owned(), 1)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn off_boarding_turns_a_person_s_supply_off_and_revokes_their_call_key() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");

  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at", "owner_account")
         VALUES ('theirs', 'theirs', 90, 1, 'directory', '/logins/theirs', 0, 7),
                ('others', 'others', 91, 1, 'directory', '/logins/others', 0, 8)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "personal_supply" ("owner_account", "road", "secret", "place", "added_at")
         VALUES (7, 'deepseek', 'ds', 1, 0), (7, 'openrouter', 'or', 1, 0), (8, 'deepseek', 'ds', 1, 0)"#,
      vec![],
    )
    .await;

  let key = call_keys::ensure(scratch.db(), &data, 7).await.unwrap();
  let port = front(ground_on(scratch.db().clone(), home.path())).await;

  assert_eq!(
    refused(client(port, Some(("7", "member"))).off_board_person(7).await),
    (403, "forbidden".to_owned())
  );

  let done = client(port, None).off_board_person(7).await.unwrap();

  assert_eq!((done.accounts, done.keys), (1, 2));
  assert!(!key.exists());
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "gateway_call_keys" WHERE "account" = 7 AND "revoked_at" IS NOT NULL"#,
        vec![]
      )
      .await,
    1
  );

  // The other person's supply stands as it stood.
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "claude_accounts" WHERE "enabled" = 1 AND "owner_account" IS NOT NULL"#,
        vec![]
      )
      .await,
    1
  );
  assert_eq!(
    scratch.count(r#"SELECT COUNT(*) FROM "personal_supply" WHERE "enabled""#, vec![]).await,
    1
  );
}

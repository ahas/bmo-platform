//! The routes a consumer holding the rooms asks beside a turn, asked of a running server in the shapes that
//! consumer sends: a supervisor's row stood up and taken out with its reset point, a save of its
//! settings naming a field none takes, agency's card and login settings kept and refused, why a rest that has just
//! passed silenced the deployment, words pushed into a session's working turn, and a run of the ledger read
//! by its id and listed by its search beside its opening and closing, on a store the server settled itself.

use serde_json::{Value, json};

use crate::support::rows::{repository, supervisor};
use crate::support::scratch::Scratch;
use crate::support::standing;

async fn get(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .get(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn patch(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .patch(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn delete(port: u16, token: &str, path: &str) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .delete(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

async fn post(port: u16, token: &str, path: &str, body: Value) -> (u16, Value) {
  let answer = reqwest::Client::new()
    .post(format!("http://127.0.0.1:{port}{path}"))
    .bearer_auth(token)
    .json(&body)
    .send()
    .await
    .unwrap();

  (answer.status().as_u16(), answer.json().await.unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_the_calls_a_room_holder_makes_beside_its_turns() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();
  repository(scratch.db(), "helper", &home.path().join("helper").to_string_lossy(), &[]).await;
  supervisor(scratch.db(), "chat-agent", &[]).await;

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let added = json!({
    "name": "supervisor-a",
    "label": "Supervisor A",
    "provider": "claude-code",
    "model": "m",
    "repository": "helper",
  });

  // A supervisor's row stood up from the fields the console's dialog gave, its order the provider and
  // model it names alone, and refused once its name is taken or its repository is none.
  let (status, registered) = post(port, token, "/supervisors", added.clone()).await;

  assert_eq!(
    (status, registered["supervisor"]["name"].as_str()),
    (201, Some("supervisor-a")),
    "{registered}"
  );
  assert_eq!(
    scratch
      .count(
        r#"SELECT count(*) FROM "supervisors" WHERE "name" = 'supervisor-a'
           AND "session_scope" = 'room' AND "turn_shape" = 'pieces'
           AND "provider" = 'claude-code' AND "model" = 'm'
           AND "repository" = 'helper' AND "brain_folder" = 'agents/supervisor-a' AND "builtin" = 0"#,
        vec![],
      )
      .await,
    1
  );

  let (status, taken) = post(port, token, "/supervisors", added.clone()).await;

  assert_eq!((status, taken["code"].as_str()), (409, Some("name-taken")), "{taken}");

  let mut workerless = added.clone();

  workerless["name"] = json!("supervisor-b");
  workerless["repository"] = json!("chat-agent");

  let (status, refused) = post(port, token, "/supervisors", workerless).await;

  assert_eq!((status, refused["field"].as_str()), (400, Some("repository")), "{refused}");

  // Its settings read whole, its room sessions none yet.
  let (status, settings) = get(port, token, "/supervisors/supervisor-a/settings").await;

  assert_eq!((status, settings["agent"].as_str()), (200, Some("supervisor-a")), "{settings}");
  assert_eq!(settings["turnShape"], "pieces");
  assert_eq!(get(port, token, "/supervisors/nobody/settings").await.0, 404);

  // Agency's own settings naming the DeepSeek top-up's card and login secrets: none at first, a secret's name
  // kept apart from the other's, a name reaching past the secrets' folder refused by its field, and null
  // letting it go.
  for (field, other, name) in [
    ("deepseekCardSecret", "deepseekLoginSecret", "saved-card"),
    ("deepseekLoginSecret", "deepseekCardSecret", "saved-login"),
  ] {
    let (status, switches) = get(port, token, "/settings").await;

    assert_eq!((status, &switches[field]), (200, &Value::Null), "{switches}");

    let (status, switches) = patch(port, token, "/settings", json!({ field: name })).await;

    assert_eq!((status, switches[field].as_str()), (200, Some(name)), "{switches}");
    assert_eq!(switches[other], Value::Null, "{switches}");

    for wrong in ["../agency.env", "folder/saved", "saved/", "..", ""] {
      let (status, refused) = patch(port, token, "/settings", json!({ field: wrong })).await;

      assert_eq!((status, refused["field"].as_str()), (400, Some(field)), "{wrong}: {refused}");
    }

    assert_eq!(get(port, token, "/settings").await.1[field], name);

    let (_, switches) = patch(port, token, "/settings", json!({ field: null })).await;

    assert_eq!(switches[field], Value::Null);
  }

  // The minutes between two flushes of the deploy queue: the policy's at first, the owner's once set, anything but
  // a positive whole number refused by its field, and null returning to the policy's.
  let policy = policy::job_notices::JOB_NOTICES.deploy_flush_minutes;

  assert_eq!(get(port, token, "/settings").await.1["deployFlushMinutes"], policy);

  let (status, switches) = patch(port, token, "/settings", json!({ "deployFlushMinutes": 15 })).await;

  assert_eq!((status, &switches["deployFlushMinutes"]), (200, &json!(15)), "{switches}");

  for wrong in [json!(0), json!(-5), json!(1.5), json!("15")] {
    let (status, refused) = patch(port, token, "/settings", json!({ "deployFlushMinutes": wrong })).await;

    assert_eq!(
      (status, refused["field"].as_str()),
      (400, Some("deployFlushMinutes")),
      "{wrong}: {refused}"
    );
  }

  let (_, switches) = patch(port, token, "/settings", json!({ "deployFlushMinutes": null })).await;

  assert_eq!(switches["deployFlushMinutes"], policy);

  // A reset point is kept for it, which its removal takes with it.
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('reset_point:supervisor-a', '2000')"#,
      vec![],
    )
    .await;

  // A save naming a field no save takes — the recollectors' model among them — is refused by that field,
  // and nothing is written.
  let (status, refused) = patch(
    port,
    token,
    "/supervisors/supervisor-a/settings",
    json!({ "recollectorModel": "local-model" }),
  )
  .await;

  assert_eq!((status, refused["field"].as_str()), (400, Some("recollectorModel")), "{refused}");

  for retired in ["pieces", "ownPieces"] {
    let (status, refused) =
      patch(port, token, "/supervisors/supervisor-a/settings", json!({ retired: [] })).await;

    assert_eq!((status, refused["field"].as_str()), (400, Some(retired)), "{refused}");
  }

  assert_eq!(
    scratch
      .count(r#"SELECT count(*) FROM "supervisor_settings" WHERE "agent" = 'supervisor-a'"#, vec![])
      .await,
    0
  );

  // Words name the turn they are pushed into, and a turn the session carries none of is not there.
  assert_eq!(
    post(port, token, "/sessions/supervisor-a@-7/turns/1/inputs", json!({ "words": "more" })).await,
    (404, json!({ "code": "unknown-turn", "error": "no turn 1 is going in supervisor-a@-7" }))
  );
  assert_eq!(
    post(port, token, "/sessions/nobody@-7/turns/1/inputs", json!({ "words": "more" }))
      .await
      .1["code"],
    "unknown-session"
  );

  // Taken out again with everything of its own; one the deployment seeds, or no supervisor, is refused.
  let (status, _) = delete(port, token, "/supervisors/supervisor-a").await;

  assert_eq!(status, 204);
  assert_eq!(
    scratch
      .count(
        r#"SELECT (SELECT count(*) FROM "supervisors" WHERE "name" = 'supervisor-a')
           + (SELECT count(*) FROM "settings" WHERE "key" = 'reset_point:supervisor-a')"#,
        vec![],
      )
      .await,
    0
  );

  let (status, _) = delete(port, token, "/supervisors/helper").await;

  assert_eq!(status, 404);

  // The service's outage rest past its end: cleared as it is read, and told once.
  scratch
    .run(
      r#"INSERT INTO "settings" ("key", "value") VALUES ('rest_until', '1000'), ('rest_kind', 'outage')"#,
      vec![],
    )
    .await;
  assert_eq!(
    patch(port, token, "/rest", json!({ "clearPassed": true })).await,
    (200, json!({ "resting": false, "until": null, "kind": "outage", "accountId": null }))
  );
  assert_eq!(
    patch(port, token, "/rest", json!({ "clearPassed": true })).await,
    (200, json!({ "resting": false, "until": null, "kind": null, "accountId": null }))
  );
  assert_eq!(
    scratch
      .count(r#"SELECT count(*) FROM "settings" WHERE "key" LIKE 'rest\_%'"#, vec![])
      .await,
    0
  );

  // An outage written with its end in milliseconds is kept and answered in seconds, as what stands; a second
  // one while it goes on writes nothing and answers the first.
  let soon =
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64 + 3600;
  let written = patch(port, token, "/rest", json!({ "until": soon * 1000, "kind": "outage" })).await;

  assert_eq!(
    written,
    (
      200,
      json!({
        "resting": true, "until": soon, "kind": "outage", "accountId": null, "written": true, "silenced": true,
      })
    )
  );
  assert_eq!(
    patch(port, token, "/rest", json!({ "until": soon + 60, "kind": "outage" })).await.1,
    json!({ "resting": true, "until": soon, "kind": "outage", "accountId": null, "written": false, "silenced": false })
  );
  assert_eq!(
    get(port, token, "/rest").await,
    (200, json!({ "resting": true, "until": soon, "kind": "outage" }))
  );

  // Ended by hand: nothing rests after it, and the end of the silence it told is answered once.
  assert_eq!(
    delete(port, token, "/rest").await,
    (200, json!({ "resting": false, "until": null, "kind": null, "ended": true }))
  );
  assert_eq!(patch(port, token, "/rest", json!({ "clearPassed": true })).await.1["kind"], "outage");
  assert_eq!(patch(port, token, "/rest", json!({ "clearPassed": true })).await.1["kind"], Value::Null);
  assert_eq!(
    delete(port, token, "/rest").await,
    (200, json!({ "resting": false, "until": null, "kind": null, "ended": false }))
  );
  assert_eq!(get(port, token, "/rest/passed").await.0, 404);

  // A run of the ledger, opened and closed in the shapes the job unit sends and read as the room holder reads
  // it; a run with no row is nothing held, and a static path beside the id's is still its own route.
  let (status, opened) = post(
    port,
    token,
    "/runs",
    json!({
      "agent": "helper",
      "chat_id": null,
      "unit": "unit-1",
      "depth": 2,
      "cap_ms": 60_000,
      "started_at": 4_000,
      "started_by": "chat:-7",
      "search_id": "12",
    }),
  )
  .await;

  assert_eq!(status, 201, "{opened}");

  let id = opened["id"].as_i64().unwrap();
  let read = format!("/runs/{id}");

  let (status, read_run) = get(port, token, &read).await;

  assert_eq!(status, 200);
  assert_eq!(read_run["id"], json!(id));
  assert_eq!(read_run["agentName"], json!("helper"));
  assert_eq!(read_run["unitName"], json!("unit-1"));
  assert_eq!(read_run["state"], json!("open"));
  assert_eq!(read_run["startedAt"], json!(4_000));

  let closing = json!({ "finishedAt": 9_000, "outcome": "done", "contextTokens": 30_000 });

  assert_eq!(patch(port, token, &read, closing.clone()).await.0, 200);
  assert_eq!(patch(port, token, &read, json!({ "finishedAt": 9_500, "outcome": "cut" })).await.0, 409);

  let (_, closed) = get(port, token, &read).await;

  assert_eq!(
    (&closed["outcome"], &closed["contextTokens"], &closed["state"]),
    (&json!("done"), &json!(30_000), &json!("closed")),
    "the first closing is kept"
  );

  let (status, numbered) = post(
    port,
    token,
    "/runs",
    json!({ "agent": "helper", "unit": "unit-2", "depth": 0, "started_at": 5_000, "search_id": 12 }),
  )
  .await;

  assert_eq!(status, 201, "{numbered}");
  assert_eq!(
    scratch.count(r#"SELECT count(*) FROM "runs" WHERE "search_id" = '12'"#, vec![]).await,
    2,
    "a search named by its number or its text is kept the same"
  );
  let (_, searched) = get(port, token, "/runs?search=12").await;
  let ids: Vec<i64> = searched["items"]
    .as_array()
    .unwrap()
    .iter()
    .filter_map(|run| run["id"].as_i64())
    .collect();

  assert_eq!(
    ids,
    vec![numbered["id"].as_i64().unwrap(), id],
    "the runs that carried a search, newest first"
  );
  assert_eq!(get(port, token, "/runs?search=13").await.1["items"], json!([]));
  assert_eq!(get(port, token, "/runs").await.0, 200);

  let missing = id + 1_000;

  assert_eq!(
    get(port, token, &format!("/runs/{missing}")).await,
    (
      404,
      json!({ "code": "not-found", "error": format!("the ledger holds no run {missing}") })
    )
  );
  assert_eq!(patch(port, token, &format!("/runs/{missing}"), closing).await.1["code"], "not-found");
  assert_eq!(post(port, token, "/runs", json!({ "agent": "helper" })).await.0, 400);
  assert_eq!(get(port, token, &read).await.0, 200);
}

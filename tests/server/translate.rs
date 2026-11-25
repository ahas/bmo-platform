//! The translator runner (docs/launch-lines.md, 「Translate runner」; docs/api.md, 「translations」): its system
//! prompt is the translator role's prompt and then the purpose's instructions, its input the target's line and
//! the text, one request with thinking off on the registry's provider, its route refusing what is no
//! translation, and a translation on a registration naming a card service with no engine standing asking its
//! demand, waiting for the granted card's entry and going out on it, never moved off the registry; and a
//! translation on the registry naming no wait of its own, waited for however long the gateway takes.

use std::collections::{BTreeMap, HashMap};
use std::os::unix::fs::PermissionsExt;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::support::grounds::{OrderEntry, set_provider_order};
use agency_server::Service;
use agency_server::config::Config;
use agency_server::control::harness::{missing, offered_registrations};
use agency_server::lines::Trees;
use agency_server::places::Ground;
use agency_server::queue::settings;
use agency_server::road::{self, Ask};
use agency_server::seeds::deployment;
use agency_server::translate::{
  KEY, LINE, Purpose, gateway_request, input, noted_system, spent_of, system_prompt, too_large, translate,
};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use bmo_agency_client::TranslationTerm;
use harness_providers::Machine;
use policy::providers::claude_code::CLAUDE_CODE;
use policy::providers::registered::REGISTERED;
use policy::roles::ROLES;
use policy::run_lines::RUN_LINES;
use policy::work_keys::WORK_KEYS;
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::scratch::Scratch;
use crate::support::standing;

fn trees() -> Trees {
  let code = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));

  Trees {
    code: code.clone(),
    agents: code.join("prompts").join("agents"),
    source: code.clone(),
    data: code.clone(),
    home: code,
  }
}

#[test]
fn a_translation_s_prompt_is_the_role_s_then_its_purpose_s() {
  let role =
    std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/prompts/roles/translator.md")).unwrap();

  for (purpose, file) in [
    (Purpose::Document, "instructions.md"),
    (Purpose::Worker, "worker.md"),
    (Purpose::Record, "record.md"),
  ] {
    let own =
      std::fs::read_to_string(format!("{}/prompts/agents/translator/{file}", env!("CARGO_MANIFEST_DIR")))
        .unwrap();
    let system = system_prompt(&trees(), Some(&role), purpose).unwrap();

    assert!(system.starts_with(role.trim()), "{purpose:?}");
    assert!(system.contains(own.trim()), "{purpose:?}");
  }

  assert!(system_prompt(&trees(), None, Purpose::Record).unwrap().contains("Plain writing"));
  assert!(
    system_prompt(&trees(), None, Purpose::Document).unwrap().starts_with(
      std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/prompts/agents/translator/instructions.md"
      ))
      .unwrap()
      .trim()
    )
  );
  assert_eq!(Purpose::read("document"), Some(Purpose::Document));
  assert_eq!(Purpose::read("gate"), None);
  assert_eq!(input(" Korean ", "the text", &[]), "Translate into Korean.\n\nthe text");
  // The caller's terms stand first, as the translation model's reference translations, a term with no target
  // kept as written.
  let terms = [
    TranslationTerm {
      source: "38.6 tokens/s".into(),
      target: None,
    },
    TranslationTerm {
      source: "Reproduction plan".into(),
      target: Some("Reproduction plan".into()),
    },
    TranslationTerm {
      source: " ".into(),
      target: None,
    },
  ];
  assert_eq!(
    input("Korean", "the text", &terms),
    "Reference the following translations:\n38.6 tokens/s translates to 38.6 tokens/s\nReproduction plan \
     translates to Reproduction plan\n\nTranslate the following text into Korean. Note that you must ONLY \
     output the translated result without any additional explanation:\n\nthe text"
  );
  // The caller's note stands in the system prompt, never in the input a translation model would render.
  assert_eq!(noted_system("the translator's prompt\n", None), "the translator's prompt\n");
  assert_eq!(
    noted_system("the translator's prompt\n", Some(" Keep it to one line. ")),
    "the translator's prompt\n\nA note on the part that follows, for you alone: Keep it to one line."
  );

  // A refusal for a part past the model's window is told apart from any other failure.
  assert!(too_large(
    "the gateway answered 400: \"This model's maximum context length is 32768 tokens\""
  ));
  assert!(too_large("Prompt is too long"));
  assert!(!too_large("the gateway answered no words"));
}

#[test]
fn the_translator_is_a_runner_with_a_role_of_its_own_and_a_line_two_providers_carry() {
  let role = ROLES.role(&ROLES.translate).expect("the translator role");

  assert!(role.bundles.is_empty(), "no web, no tool");
  assert_eq!(role.prompt.as_deref(), Some("translator.md"));

  let profile = WORK_KEYS.profile(KEY).expect("the translate key");

  assert_eq!((profile.kind.as_str(), profile.line.as_str()), ("runner", LINE));
  assert_eq!(profile.default_route_mode, "registered", "the registered model first");
  assert!(RUN_LINES.line(LINE).is_some());
  assert!(REGISTERED.declared.carries(LINE) && CLAUDE_CODE.declared.carries(LINE));
}

#[test]
fn on_the_registry_s_provider_a_translation_is_one_request_with_thinking_off() {
  let request = gateway_request("a-model", "the system", "Translate into English.\n\nthe text");

  assert_eq!(request["model"], "a-model");
  assert_eq!(
    request["messages"],
    json!([
      { "role": "system", "content": "the system" },
      { "role": "user", "content": "Translate into English.\n\nthe text" },
    ])
  );
  assert_eq!(request["chat_template_kwargs"]["enable_thinking"], false);
  assert_eq!(request["max_tokens"], 2048);
  assert_eq!(request["temperature"], 0.3);
}

#[tokio::test(flavor = "multi_thread")]
async fn the_route_refuses_what_is_no_translation() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";

  std::fs::create_dir_all(&data).unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let refused = |body: Value| {
    let port = server.port;

    async move {
      let answer = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/translation-runs"))
        .bearer_auth(token)
        .json(&body)
        .timeout(Duration::from_secs(60))
        .send()
        .await
        .unwrap();

      (answer.status().as_u16(), answer.json::<Value>().await.unwrap_or(Value::Null))
    }
  };

  let (status, said) = refused(json!({ "purpose": "gate", "language": "English", "text": "x" })).await;

  assert_eq!((status, said["code"].as_str()), (400, Some("bad-request")), "{said}");

  let (status, said) = refused(json!({ "purpose": "document", "language": "English", "text": " " })).await;

  assert_eq!((status, said["code"].as_str()), (400, Some("bad-request")), "{said}");
}

/// The stand-in master and gateway's memory: the demands asked, how often each was read before its grant,
/// the ones given back, how often the gateway was read after the grant before it lists the granted card's
/// entry, and every model a request named.
#[derive(Default)]
struct Engine {
  asked: Vec<Value>,
  reads: BTreeMap<i64, u32>,
  granted: bool,
  given_back: Vec<i64>,
  lists_after_grant: u32,
  sent: Vec<String>,
  inputs: Vec<String>,
}

type Shared = Arc<Mutex<Engine>>;

// How many reads a demand waits before its card is granted, and how many reads of the gateway's list the
// launcher's beat takes after the grant to lay the started instance into its table.
const GRANT_AFTER: u32 = 2;
const BEAT_AFTER: u32 = 2;

async fn ask(State(shared): State<Shared>, Json(body): Json<Value>) -> impl IntoResponse {
  let mut engine = shared.lock().unwrap();
  let id = engine.asked.len() as i64 + 1;

  engine.asked.push(body.clone());
  (
    StatusCode::ACCEPTED,
    Json(json!({
      "id": id, "serviceName": body["serviceName"], "count": 1, "cards": [], "state": "waiting",
      "reason": { "code": "starting", "text": "The engine is starting." }, "waitedMs": 0,
    })),
  )
}

async fn read(State(shared): State<Shared>, Path(id): Path<i64>) -> axum::response::Response {
  let mut engine = shared.lock().unwrap();

  if engine.given_back.contains(&id) || id > engine.asked.len() as i64 {
    return StatusCode::NOT_FOUND.into_response();
  }

  let reads = {
    let reads = engine.reads.entry(id).or_default();
    *reads += 1;
    *reads
  };

  if reads < GRANT_AFTER {
    return Json(json!({
      "id": id, "serviceName": "gguf", "count": 1, "cards": [], "state": "waiting",
      "reason": { "code": "starting", "text": "The engine is starting." }, "waitedMs": 0,
    }))
    .into_response();
  }

  engine.granted = true;
  Json(json!({
    "id": id, "serviceName": "gguf", "count": 1, "state": "granted",
    "cards": [{ "nodeName": "n1", "address": "http://[::1]:9/v1/chat/completions", "keyFile": null }],
  }))
  .into_response()
}

async fn give_back(State(shared): State<Shared>, Path(id): Path<i64>) -> StatusCode {
  shared.lock().unwrap().given_back.push(id);
  StatusCode::NO_CONTENT
}

// The gateway's list: nothing while no instance stands, the granted card's entry once the launcher's beat
// after the grant has laid it in.
async fn listed(State(shared): State<Shared>) -> Json<Value> {
  let mut engine = shared.lock().unwrap();

  if engine.granted {
    engine.lists_after_grant += 1;
  }

  Json(match engine.lists_after_grant > BEAT_AFTER {
    true => json!({ "data": [{ "id": "hy" }, { "id": "hy@n1" }] }),
    false => json!({ "data": [] }),
  })
}

// A request is answered only on an entry the gateway's table holds, as the gateway refuses any other.
async fn completed(State(shared): State<Shared>, Json(body): Json<Value>) -> axum::response::Response {
  let mut engine = shared.lock().unwrap();
  let model = body["model"].as_str().unwrap_or_default().to_owned();

  engine.sent.push(model.clone());
  engine
    .inputs
    .push(body["messages"][1]["content"].as_str().unwrap_or_default().to_owned());

  if engine.lists_after_grant <= BEAT_AFTER {
    return (
      StatusCode::BAD_REQUEST,
      Json(json!({ "error": format!("no deployment named {model}") })),
    )
      .into_response();
  }

  Json(json!({ "choices": [{ "message": { "content": "the text, rendered" } }] })).into_response()
}

async fn stand_in() -> (String, Shared) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let shared: Shared = Arc::default();
  let app = axum::Router::new()
    .route("/demands", post(ask))
    .route("/demands/{id}", get(read).delete(give_back))
    .route("/v1/models", get(listed))
    .route("/v1/chat/completions", post(completed))
    .with_state(shared.clone());

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (base, shared)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_translation_on_a_card_service_with_no_engine_standing_asks_its_demand_and_never_moves() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let (base, shared) = stand_in().await;
  let variables: HashMap<String, String> = [("AGENCY_ARBITER_URL".to_owned(), base.clone())].into();
  let service = Service::new(db.clone(), Config::read(|name| variables.get(name).cloned()).unwrap());
  let at = |path: &std::path::Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/nowhere".to_owned()),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
      ("BMO_LLM_GATEWAY_URL".to_owned(), base.clone()),
    ],
    data.clone(),
  );
  let ground = Ground::new(&service, &machine, None);
  let key = data.join("env").join(&REGISTERED.presence.key_file);

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "a key\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
  store::seed::settle(db, &deployment(&machine)).await.unwrap();
  set_provider_order(
    db,
    &[
      OrderEntry {
        name: "claude-code".into(),
        enabled: true,
      },
      OrderEntry {
        name: "registered".into(),
        enabled: true,
      },
    ],
  )
  .await
  .unwrap();

  // The registry holds one registration, naming a card service and so no address of its own.
  scratch.run(r#"DELETE FROM "registered_models""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
            "thinking", "walked", "added_at", "keeps_research", "card_service", "role_files")
         VALUES ('hy', 'hy', 'served', 32768, 2048, 0, 0, 1, 0, 'gguf', 0)"#,
      vec![],
    )
    .await;
  // The translator runs on the one registration its order names.
  scratch
    .run(
      r#"UPDATE "roles" SET "provider_order" = '[{"provider":"registered","model":"hy"}]' WHERE "name" = 'translator'"#,
      vec![],
    )
    .await;
  crate::support::rows::reseated(scratch.db()).await;

  // With no instance standing the registry still offers the registration and can answer for the key.
  let registered = agency_server::control::harness::named("registered").unwrap();

  assert_eq!(offered_registrations(db).await.unwrap(), ["hy"]);
  assert!(missing(db, &machine, registered).await.unwrap().is_none());

  let translator = agency_server::agent_roles::named(db, "translator").await.unwrap().unwrap();
  let resolution =
    road::resolve(&ground, &Ask::of(road::Row::Role("translator"), &translator.line, &translator.order))
      .await
      .unwrap();

  assert_eq!(
    resolution.road().map(|road| (road.provider.as_str(), road.model.as_str())),
    Some(("registered", "hy")),
    "{resolution:?}"
  );

  let terms = [TranslationTerm {
    source: "Thing-7B".into(),
    target: None,
  }];
  let translated = translate(&ground, "document", "English", (None, &terms), "the text").await.unwrap();

  assert_eq!(
    (translated.provider.as_str(), translated.model.as_str(), translated.text.as_str()),
    ("registered", "hy", "the text, rendered")
  );

  {
    let engine = shared.lock().unwrap();

    assert_eq!(engine.asked.len(), 1, "the translation asked its demand");
    assert_eq!(engine.asked[0]["serviceName"], "gguf");
    assert_eq!(
      engine.sent,
      ["hy@n1"],
      "sent once, to the granted card's entry, after the gateway held it"
    );
    assert_eq!(engine.given_back, [1], "the demand given back once the translation was answered");
    assert_eq!(
      engine.inputs.last().map(String::as_str),
      Some(input("English", "the text", &terms).as_str()),
      "the caller's terms reach the model in its input"
    );
  }

  let kept = settings::get(db, &format!("road_kept:{KEY}")).await.unwrap();

  assert_eq!(kept, None, "the key's road never moved");
}

// The wait headers each request named to the stand-in gateway, and how long it takes to answer one.
type Named = Arc<Mutex<Vec<(String, String)>>>;

async fn slowly(State((named, slow)): State<(Named, Duration)>, headers: HeaderMap) -> Json<Value> {
  let header =
    |name: &str| headers.get(name).and_then(|value| value.to_str().ok()).unwrap_or_default().to_owned();

  named
    .lock()
    .unwrap()
    .push((header("x-litellm-timeout"), header("x-litellm-stream-timeout")));
  tokio::time::sleep(slow).await;
  Json(json!({
    "choices": [{ "message": { "content": "the text, rendered" } }],
    "usage": { "prompt_tokens": 41, "completion_tokens": 7 },
  }))
}

// A gateway that answers each request only after `slow`.
async fn slow_gateway(slow: Duration) -> (String, Named) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let base = format!("http://{}", listener.local_addr().unwrap());
  let named: Named = Arc::default();
  let app = axum::Router::new()
    .route("/v1/chat/completions", post(slowly))
    .with_state((named.clone(), slow));

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (base, named)
}

#[tokio::test(flavor = "multi_thread")]
async fn a_translation_on_the_registry_names_no_wait_and_is_waited_for_however_long_the_gateway_takes() {
  let scratch = Scratch::new().await;
  let db = scratch.db();
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let (base, named) = slow_gateway(Duration::from_secs(3)).await;
  let service = Service::new(db.clone(), Config::read(|_| None).unwrap());
  let at = |path: &std::path::Path| path.to_string_lossy().into_owned();
  let machine = Machine::with_env(
    [
      ("HOME".to_owned(), at(home.path())),
      ("PATH".to_owned(), "/nowhere".to_owned()),
      ("BMO_DEPLOY_DIR".to_owned(), env!("CARGO_MANIFEST_DIR").to_owned()),
      ("BMO_LLM_GATEWAY_URL".to_owned(), base.clone()),
    ],
    data.clone(),
  );
  let ground = Ground::new(&service, &machine, None);
  let key = data.join("env").join(&REGISTERED.presence.key_file);

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "a key\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
  store::seed::settle(db, &deployment(&machine)).await.unwrap();
  set_provider_order(
    db,
    &[OrderEntry {
      name: "registered".into(),
      enabled: true,
    }],
  )
  .await
  .unwrap();

  // One registration on an address, whose request wait is shorter than the gateway takes to answer: the
  // registration's wait is the gateway's entry's, never agency's.
  scratch.run(r#"DELETE FROM "registered_models""#, vec![]).await;
  scratch
    .run(
      r#"INSERT INTO "registered_models" ("name", "label", "model", "context_tokens", "answer_tokens",
            "thinking", "walked", "added_at", "keeps_research")
         VALUES ('hy', 'hy', 'served', 32768, 2048, 0, 0, 1, 0)"#,
      vec![],
    )
    .await;
  scratch
    .run(
      r#"INSERT INTO "registered_model_addresses" ("registration", "node", "position", "url", "key_file", "key_owned")
         VALUES ('hy', 'n1', 1, 'http://[::1]:9/v1/chat/completions', NULL, 0)"#,
      vec![],
    )
    .await;
  // The translator runs on the one registration its order names.
  scratch
    .run(
      r#"UPDATE "roles" SET "provider_order" = '[{"provider":"registered","model":"hy"}]' WHERE "name" = 'translator'"#,
      vec![],
    )
    .await;
  crate::support::rows::reseated(scratch.db()).await;

  let translated = translate(&ground, "document", "English", (None, &[]), "the text").await.unwrap();

  assert_eq!(translated.text, "the text, rendered", "waited for until the gateway answered");
  assert_eq!(*named.lock().unwrap(), [(String::new(), String::new())], "no wait named to the gateway");

  // The translation left a run of the translator role, closed with what the gateway counted, and its log
  // lines: the start, one note and the end, holding none of the text.
  let role = vec![ROLES.translate.clone().into()];

  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "runs" WHERE "agent" = $1 AND "unit" LIKE 'translate:%' AND "outcome" = 'answered'
             AND "finished_at" IS NOT NULL AND "input_tokens" = 41 AND "output_tokens" = 7"#,
        role.clone(),
      )
      .await,
    1
  );
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "agent_log" l JOIN "runs" r ON r."agent" = l."agent" AND r."unit" = l."run"
           WHERE l."agent" = $1 AND l."kind" IN ('start', 'note', 'end')"#,
        role.clone(),
      )
      .await,
    3
  );
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "agent_log" WHERE "agent" = $1 AND "text" LIKE '%the text%'"#,
        role.clone()
      )
      .await,
    0,
    "none of the text translated is written down"
  );
  assert_eq!(
    scratch
      .count(
        r#"SELECT COUNT(*) FROM "agent_log" WHERE "agent" = $1 AND "kind" = 'note'
             AND "text" = 'document · English · ' || $2 || ' characters in, 18 back'"#,
        vec![
          ROLES.translate.clone().into(),
          input("English", "the text", &[]).chars().count().to_string().into()
        ],
      )
      .await,
    1
  );
}

#[test]
fn what_a_translation_spent_is_what_the_gateway_s_answer_counts() {
  let spent = spent_of(&json!({ "usage": { "prompt_tokens": 41, "completion_tokens": 7 } })).unwrap();

  assert_eq!((spent.input_tokens, spent.output_tokens, spent.total()), (41, 7, 48));
  assert_eq!(
    spent_of(&json!({ "usage": { "prompt_tokens": 3 } })).map(|spent| spent.total()),
    Some(3)
  );
  assert_eq!(spent_of(&json!({ "choices": [] })), None);
}

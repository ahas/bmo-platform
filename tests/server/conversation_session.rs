//! A conversation the caller opened over the session contract (docs/api.md, 「Sessions」): started on the
//! provider and model it names under a number of its own, its state those and its folder at the first
//! generation, named in the list until it ends, refusing a fold, a work turn's parts and a provider this
//! build does not carry, and gone once ended; and the provider and model it stands on switched by its
//! change route, each left as it stands where the caller names only the other.

use std::collections::HashMap;
use std::sync::Arc;

use agency_server::Service;
use agency_server::config::Config;
use agency_server::herdr_backend::{HerdrSessions, conversation_id, is_conversation};
use agency_server::places::{Ground, Places};
use agency_server::seeds::deployment;
use agent_session::{
  Conversation, FaultKind, Resume, Scope, SessionId, Sessions, Start, Turn, TurnContext, WorkParts,
};

use crate::support::rows::{repository, supervisor};
use crate::support::scratch::Scratch;

fn machine(home: &std::path::Path) -> harness_providers::Machine {
  let at = |path: std::path::PathBuf| path.to_string_lossy().into_owned();

  harness_providers::Machine::with_env(
    [
      ("HOME".to_owned(), at(home.to_path_buf())),
      ("PATH".to_owned(), "/usr/bin:/bin".to_owned()),
    ],
    home.join("data"),
  )
}

fn ground(scratch: &Scratch, machine: &harness_providers::Machine) -> Ground {
  let variables: HashMap<String, String> = HashMap::new();
  let service =
    Service::new(scratch.db().clone(), Config::read(|name| variables.get(name).cloned()).unwrap());

  Ground::new(&service, machine, None)
}

fn conversation(provider: &str, model: &str, cwd: &str) -> Conversation {
  Conversation {
    provider: provider.into(),
    model: model.into(),
    cwd: cwd.into(),
  }
}

fn start(agent: &str, conversation: Conversation) -> Start {
  Start {
    agent: agent.into(),
    scope: Scope::Conversation(conversation),
  }
}

async fn agents(scratch: &Scratch) {
  supervisor(scratch.db(), "chat-agent", &[]).await;
  repository(scratch.db(), "worker-a", "/checkouts/worker-a", &[]).await;
}

#[test]
fn a_conversations_id_names_its_agent_and_its_number() {
  assert_eq!(conversation_id("chat-agent", 3), SessionId("chat-agent@conversation:3".into()));
  assert!(is_conversation(&conversation_id("chat-agent", 3)));
  assert!(!is_conversation(&SessionId("chat-agent@conversation".into())));
  assert!(!is_conversation(&SessionId("chat-agent@apart:3".into())));
  assert!(!is_conversation(&SessionId("chat-agent@-7".into())));
}

#[tokio::test]
async fn a_conversation_stands_on_the_provider_and_model_it_names_and_switches_both() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let machine = machine(home.path());

  store::seed::settle(scratch.db(), &deployment(&machine)).await.unwrap();

  agents(&scratch).await;

  let places = Places::new(ground(&scratch, &machine), Arc::default());
  let sessions = HerdrSessions::new(places);
  let first = sessions
    .start(start("chat-agent", conversation("openrouter", "deepseek/deepseek-chat", "/tmp/run-a")))
    .await
    .unwrap();

  assert_eq!(first.id, SessionId("chat-agent@conversation:1".into()));

  let state = sessions.read_state(&first.id, None).await.unwrap();

  assert_eq!(
    (state.provider.as_str(), state.model.as_str(), state.place.as_str(), state.generation),
    ("openrouter", "deepseek/deepseek-chat", "/tmp/run-a", 1)
  );
  assert_eq!((state.context, state.run, state.went_out), (None, None, None));

  assert!(sessions.list().await.unwrap().contains(&first));

  let second = sessions
    .start(start("chat-agent", conversation("codex", "gpt-5", "/tmp/run-b")))
    .await
    .unwrap();

  assert_eq!(second.id, SessionId("chat-agent@conversation:2".into()));
  assert!(sessions.list().await.unwrap().contains(&second));

  // The provider and model move together, then one alone.
  let moved = sessions.switch(&first.id, Some("codex"), Some("gpt-5")).await.unwrap();

  assert_eq!(moved.scope, Scope::Conversation(conversation("codex", "gpt-5", "/tmp/run-a")));

  let state = sessions.read_state(&first.id, None).await.unwrap();

  assert_eq!((state.provider.as_str(), state.model.as_str()), ("codex", "gpt-5"));

  sessions.switch(&first.id, None, Some("gpt-5-mini")).await.unwrap();

  let state = sessions.read_state(&first.id, None).await.unwrap();

  assert_eq!(
    (state.provider.as_str(), state.model.as_str(), state.place.as_str()),
    ("codex", "gpt-5-mini", "/tmp/run-a")
  );

  assert_eq!(
    sessions.switch(&first.id, Some("no-such-provider"), None).await.unwrap_err().kind,
    FaultKind::Refused
  );
  assert_eq!(
    sessions.fold(&first.id).await.unwrap_err().kind,
    FaultKind::Refused,
    "a conversation is compacted by its road, not by a fold route"
  );

  let work = Turn {
    session: first.id.clone(),
    prompt: "do the work".into(),
    context: TurnContext {
      work: Some(WorkParts {
        provider: "claude-code".into(),
        model: "claude-sonnet-5".into(),
        mapping: agent_session::Mapping {
          model: "claude-sonnet-5".into(),
          effort: None,
          tier: None,
          answer_tokens: None,
        },
        resume: Resume::Fresh,
        cwd: "/tmp/run-a".into(),
        name: "job:1".into(),
        title: "a job".into(),
        chat: 1,
        unit: None,
        user: None,
        work_key: "job-worker".into(),
        hears_words: false,
        variables: Default::default(),
      }),
      ..TurnContext::default()
    },
  };

  assert_eq!(
    sessions.send_turn(work).await.unwrap_err().kind,
    FaultKind::Refused,
    "a conversation's turn carries no work parts"
  );

  sessions.end(&first.id).await.unwrap();

  assert_eq!(sessions.read_state(&first.id, None).await.unwrap_err().kind, FaultKind::Unknown);
  assert_eq!(sessions.end(&first.id).await.unwrap_err().kind, FaultKind::Unknown);
  assert!(!sessions.list().await.unwrap().contains(&first));
  assert!(sessions.list().await.unwrap().contains(&second));

  assert_eq!(
    sessions
      .start(start("worker-a", conversation("codex", "gpt-5", "/tmp/run-c")))
      .await
      .unwrap_err()
      .kind,
    FaultKind::Refused,
    "a conversation is a supervisor's"
  );
  assert_eq!(
    sessions
      .start(start("chat-agent", conversation("nope", "a-model", "/tmp/run-c")))
      .await
      .unwrap_err()
      .kind,
    FaultKind::Refused
  );
  assert_eq!(
    sessions
      .start(start("a@b", conversation("codex", "gpt-5", "/tmp/run-c")))
      .await
      .unwrap_err()
      .kind,
    FaultKind::Refused
  );
}

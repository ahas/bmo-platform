//! A backend of a test's own over the session contract, standing in for herdr where a test carries a job's turns:
//! what it was asked to start and handed, and the answers it gives in order.

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use agent_session::{
  Fault, FaultKind, Outcome, Reply, Scope, Session, SessionId, Sessions, Start, State, Turn, TurnEnd,
};

/// A backend of the test's own over the session contract: it keeps every session it was asked to start and
/// every turn it was handed, and answers each turn, in order, with the words and the harness session given. Where
/// it is given the key every command stands under now, a turn naming a session opened under another key is put
/// back before anything goes in, as a line puts back a turn whose prefix moved.
#[derive(Default)]
pub struct StandIn {
  pub started: Mutex<Vec<Start>>,
  pub sent: Mutex<Vec<Turn>>,
  pub answers: Mutex<VecDeque<(String, String)>>,
  pub prefix: Mutex<Option<String>>,
}

impl StandIn {
  pub fn answering(answers: &[(&str, &str)]) -> Arc<Self> {
    Arc::new(Self {
      answers: Mutex::new(
        answers
          .iter()
          .map(|(words, session)| ((*words).to_owned(), (*session).to_owned()))
          .collect(),
      ),
      ..Self::default()
    })
  }

  // Each turn handed, with the session it was started on: its road and the harness session it resumes.
  pub fn turns(&self) -> Vec<(String, Option<String>, String)> {
    let started = self.started.lock().unwrap();

    self
      .sent
      .lock()
      .unwrap()
      .iter()
      .map(|turn| {
        let number: usize = turn.session.0.trim_start_matches("stand-in-").parse().unwrap();
        let Scope::Apart(apart) = &started[number].scope else {
          panic!("a job's turn goes on an apart session")
        };

        (apart.provider.clone(), apart.resume.clone(), turn.prompt.clone())
      })
      .collect()
  }
}

impl Sessions for StandIn {
  async fn start(&self, request: Start) -> Result<Session, Fault> {
    let mut started = self.started.lock().unwrap();
    let id = SessionId(format!("stand-in-{}", started.len()));

    started.push(request.clone());

    Ok(Session {
      id,
      agent: request.agent,
      scope: request.scope,
    })
  }

  async fn switch(&self, _: &SessionId, _: Option<&str>, _: Option<&str>) -> Result<Session, Fault> {
    Err(Fault::new(FaultKind::Refused, "the stand-in does not switch"))
  }

  async fn send_turn(&self, turn: Turn) -> Result<TurnEnd, Fault> {
    let prefix = self.prefix.lock().unwrap().clone();
    let number: usize = turn.session.0.trim_start_matches("stand-in-").parse().unwrap_or(usize::MAX);
    let moved = self.started.lock().unwrap().get(number).is_some_and(|start| match &start.scope {
      Scope::Apart(apart) => {
        apart.resume.is_some()
          && apart.resume_prefix.is_some()
          && prefix.is_some()
          && apart.resume_prefix != prefix
      }
      _ => false,
    });

    self.sent.lock().unwrap().push(turn);

    if moved {
      return Ok(TurnEnd::of("job-run", Outcome::Reprepared));
    }

    let (words, session) = self
      .answers
      .lock()
      .unwrap()
      .pop_front()
      .ok_or_else(|| Fault::new(FaultKind::Refused, "no answer left"))?;
    let reply = Reply {
      mode: "stand-in".into(),
      model: None,
      effort: None,
      text: words.clone(),
      session_id: Some(session),
      account: None,
      context_ratio: None,
      context_tokens: None,
      session_usage: None,
      spend: vec![],
      live_tasks: vec![],
      workflows: vec![],
      upstream: None,
      request_usages: None,
    };

    Ok(TurnEnd {
      reply: Some(Box::new(reply)),
      prefix,
      ..TurnEnd::answered("job-run", words)
    })
  }

  async fn read_state(&self, _: &SessionId, _: Option<&str>) -> Result<State, Fault> {
    Err(Fault::new(FaultKind::Refused, "the stand-in keeps no state"))
  }

  async fn interrupt(&self, _: &SessionId, _: i64) -> Result<(), Fault> {
    Ok(())
  }

  async fn fold(&self, _: &SessionId) -> Result<(), Fault> {
    Ok(())
  }

  async fn end(&self, _: &SessionId) -> Result<(), Fault> {
    Ok(())
  }

  async fn list(&self) -> Result<Vec<Session>, Fault> {
    Ok(Vec::new())
  }
}

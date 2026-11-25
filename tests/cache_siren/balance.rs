//! The balance reader against a stand-in endpoint: each account asked with its own key and its balance kept
//! under it alone, an account refused or with no key keeping what it had, an endpoint slow to answer waited
//! for, and the reader declaring its source's provider, cadence and span.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::Router;
use axum::extract::Request;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use cache_siren::balance::{Asking, BalanceAsk, BalanceReader, BalanceSource, Requests};
use cache_siren::{Balance, Keeping, Reader, Slot};
use serde_json::{Value, json};
use tokio::net::TcpListener;

use crate::support::scratch::Scratch;

const NOW_MS: i64 = 1_800_000_000_000;
const NOW: i64 = NOW_MS / 1000;

// The key the stand-in refuses as unauthorised, and the one it answers only after a while.
const REFUSED: &str = "sk-refused";
const SLOW: &str = "sk-slow";

/// A source naming its asks as given, reading `left` out of an answer, and the request allowance out of the
/// second address's answer where it names one.
struct Fixed {
  url: String,
  requests_url: Option<String>,
  asks: Vec<(&'static str, Result<&'static str, &'static str>)>,
}

impl BalanceSource for Fixed {
  fn mode(&self) -> &str {
    "paid"
  }

  fn cadence(&self) -> Duration {
    Duration::from_secs(300)
  }

  fn fresh(&self) -> Duration {
    Duration::from_secs(1800)
  }

  fn asks(&self) -> Asking<'_> {
    Box::pin(async move {
      Ok(
        self
          .asks
          .iter()
          .map(|(account, key)| BalanceAsk {
            account: (*account).to_owned(),
            url: self.url.clone(),
            key: key.map(str::to_owned).map_err(str::to_owned),
            requests_url: self.requests_url.clone(),
          })
          .collect(),
      )
    })
  }

  fn balance_of(&self, answer: &Value) -> Result<Balance, String> {
    Ok(Balance {
      amount: answer["left"].as_f64().ok_or("the answer states nothing left")?,
      currency: "CNY".into(),
      requests: None,
    })
  }

  fn requests_of(&self, answer: &Value) -> Option<Requests> {
    Some(Requests {
      used: answer["used"].as_i64()?,
      limit: answer["limit"].as_i64()?,
      left: answer["left"].as_i64()?,
      scope: Some("some-requests".into()),
    })
  }
}

/// A balance endpoint answering each key with what it has left, and the authorisation each ask carried.
async fn balance_endpoint() -> (String, Arc<Mutex<Vec<String>>>) {
  let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
  let port = listener.local_addr().unwrap().port();
  let asked = Arc::new(Mutex::new(Vec::new()));
  let heard = asked.clone();
  let app = Router::new().fallback(move |request: Request| {
    let heard = heard.clone();

    async move {
      let said = request
        .headers()
        .get("authorization")
        .map(|value| value.to_str().unwrap().to_owned())
        .unwrap_or_default();

      heard.lock().unwrap().push(said.clone());

      match said.strip_prefix("Bearer ").unwrap_or_default() {
        REFUSED => StatusCode::UNAUTHORIZED.into_response(),
        SLOW => {
          tokio::time::sleep(Duration::from_millis(300)).await;
          axum::Json(json!({ "left": 3.5 })).into_response()
        }
        _ if request.uri().path() == "/requests" => {
          axum::Json(json!({ "used": 12, "limit": 50, "left": 38 })).into_response()
        }
        _ => axum::Json(json!({ "left": 42.5 })).into_response(),
      }
    }
  });

  tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });

  (format!("http://127.0.0.1:{port}/balance"), asked)
}

fn slot(account: &str) -> Slot<'_> {
  Slot {
    mode: "paid",
    account,
  }
}

#[tokio::test]
async fn each_account_is_read_with_its_own_key_and_one_that_cannot_be_asked_keeps_what_it_had() {
  let scratch = Scratch::new().await;
  let (url, asked) = balance_endpoint().await;
  let source = Fixed {
    url,
    requests_url: None,
    asks: vec![
      ("", Ok("sk-road")),
      ("refused", Ok(REFUSED)),
      ("keyless", Err("the key file is unreadable")),
      ("slow", Ok(SLOW)),
    ],
  };
  let reader = Arc::new(BalanceReader::new(Arc::new(source)));
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);
  let before = Balance {
    amount: 7.0,
    currency: "CNY".into(),
    requests: None,
  };

  for account in ["refused", "keyless", "slow"] {
    keeping.replace_balance(slot(account), &before, NOW - 60).await.unwrap();
  }

  let outcomes = reader.read_each(&keeping, NOW_MS).await.unwrap();

  assert_eq!(
    outcomes,
    [
      (
        String::new(),
        Ok(Balance {
          amount: 42.5,
          currency: "CNY".into(),
          requests: None,
        })
      ),
      ("refused".to_owned(), Err("HTTP 401".to_owned())),
      ("keyless".to_owned(), Err("the key file is unreadable".to_owned())),
      (
        "slow".to_owned(),
        Ok(Balance {
          amount: 3.5,
          currency: "CNY".into(),
          requests: None,
        })
      ),
    ]
  );
  assert_eq!(
    *asked.lock().unwrap(),
    ["Bearer sk-road", "Bearer sk-refused", "Bearer sk-slow"],
    "a key goes in its own account's ask alone, and no ask goes out without one"
  );

  let road = keeping.balance(slot("")).await.unwrap().unwrap();

  assert_eq!(road.balance.amount, 42.5);
  assert_eq!(road.seen_at, NOW);

  let slow = keeping.balance_of_account("slow").await.unwrap().unwrap();

  assert_eq!((slow.balance.amount, slow.seen_at), (3.5, NOW), "an ask slow to answer is waited for");

  for account in ["refused", "keyless"] {
    let kept = keeping.balance_of_account(account).await.unwrap().unwrap();

    assert_eq!((kept.balance, kept.seen_at), (before.clone(), NOW - 60), "{account}");
  }

  assert_eq!(keeping.balance_of_account("").await.unwrap(), None, "the road's own is no account's");
  assert_eq!(keeping.balance(slot("never")).await.unwrap(), None);
}

#[tokio::test]
async fn a_second_address_is_asked_with_the_same_key_and_its_allowance_kept_beside_the_balance() {
  let scratch = Scratch::new().await;
  let (url, asked) = balance_endpoint().await;
  let requests_url = url.replace("/balance", "/requests");
  let reader = Arc::new(BalanceReader::new(Arc::new(Fixed {
    url: url.clone(),
    requests_url: Some(requests_url),
    asks: vec![("", Ok("sk-road")), ("refused", Ok(REFUSED))],
  })));
  let keeping = Keeping::new(scratch.db().clone(), vec![reader.clone()]);
  let allowance = Requests {
    used: 12,
    limit: 50,
    left: 38,
    scope: Some("some-requests".into()),
  };

  let outcomes = reader.read_each(&keeping, NOW_MS).await.unwrap();

  assert_eq!(
    outcomes[0],
    (
      String::new(),
      Ok(Balance {
        amount: 42.5,
        currency: "CNY".into(),
        requests: Some(allowance.clone()),
      })
    )
  );
  assert_eq!(outcomes[1], ("refused".to_owned(), Err("HTTP 401".to_owned())));
  assert_eq!(
    *asked.lock().unwrap(),
    ["Bearer sk-road", "Bearer sk-road", "Bearer sk-refused"],
    "the second address is asked with the same key, and never after a balance that could not be read"
  );
  assert_eq!(keeping.balance(slot("")).await.unwrap().unwrap().balance.requests, Some(allowance));

  // A second address that cannot be read keeps the amount with no allowance, and a reading with none
  // replaces one kept before.
  let unread = Arc::new(BalanceReader::new(Arc::new(Fixed {
    url,
    requests_url: Some("http://[::1]:9/requests".into()),
    asks: vec![("", Ok("sk-road"))],
  })));

  unread.read_each(&keeping, NOW_MS).await.unwrap();

  let kept = keeping.balance(slot("")).await.unwrap().unwrap();

  assert_eq!((kept.balance.amount, kept.balance.requests), (42.5, None));
}

#[test]
fn the_reader_declares_its_source_s_provider_cadence_and_span() {
  let reader = BalanceReader::new(Arc::new(Fixed {
    url: "http://[::1]:9/".into(),
    requests_url: None,
    asks: vec![],
  }));

  assert_eq!(reader.mode(), "paid");
  assert_eq!(reader.cadence(), Duration::from_secs(300));
  assert_eq!(reader.fresh(), Duration::from_secs(1800));
}

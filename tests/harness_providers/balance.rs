//! A provider's balance as its member declares the reading: DeepSeek's and OpenRouter's answers read into
//! an amount and a currency, an answer that states none refused with why, the address a stand-in names
//! winning over the provider's own, each road's key, and an endpoint account the provider's where its
//! address stands on the balance address's origin. Codex, Claude Code and the registered models offer none.

use std::os::unix::fs::PermissionsExt;

use harness_providers::balance::{Balance, Requests};
use harness_providers::deepseek::DeepSeek;
use harness_providers::{Machine, Provider, provider};
use serde_json::json;

#[test]
fn deepseek_s_first_balance_is_the_reading() {
  let offers = DeepSeek.balance().unwrap();
  let answer = json!({
    "is_available": true,
    "balance_infos": [
      { "currency": "CNY", "total_balance": "110.25", "granted_balance": "10.00", "topped_up_balance": "100.25" },
      { "currency": "USD", "total_balance": "2.00" },
    ],
  });

  assert_eq!(
    offers.read(&answer),
    Ok(Balance {
      amount: 110.25,
      currency: "CNY".into()
    })
  );
  assert_eq!(offers.read(&json!({ "balance_infos": [] })), Err("the answer states no balance".into()));
  assert_eq!(
    offers.read(&json!({ "balance_infos": [{ "currency": "CNY" }] })),
    Err("the balance states no total".into())
  );
}

#[test]
fn openrouter_s_credits_less_its_usage_is_the_reading_in_us_dollars() {
  let offers = provider("openrouter").unwrap().balance().unwrap();

  assert_eq!(
    offers.read(&json!({ "data": { "total_credits": 15, "total_usage": 2.5 } })),
    Ok(Balance {
      amount: 12.5,
      currency: "USD".into()
    })
  );
  assert_eq!(
    offers.read(&json!({ "data": { "total_credits": 15 } })),
    Err("the answer states no total usage".into())
  );
}

#[test]
fn openrouter_states_the_day_s_allowance_of_free_model_requests_at_its_second_address() {
  let offers = provider("openrouter").unwrap().balance().unwrap();
  let home = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([], home.path());
  let stood_in = Machine::with_env(
    [("BMO_OPENROUTER_REQUESTS_URL".to_owned(), "http://[::1]:9/key".to_owned())],
    home.path(),
  );

  assert_eq!(offers.requests_url(&machine).as_deref(), Some("https://openrouter.ai/api/v1/key"));
  assert_eq!(offers.requests_url(&stood_in).as_deref(), Some("http://[::1]:9/key"));
  assert_eq!(
    offers.requests(&json!({
      "data": { "is_free_tier": false, "free_model_daily_requests": { "used": 12, "limit": 1000, "remaining": 988 } }
    })),
    Some(Requests {
      used: 12,
      limit: 1000,
      left: 988,
      scope: Some("free-models".into()),
    })
  );
  assert_eq!(offers.requests(&json!({ "data": { "is_free_tier": true } })), None);
  assert_eq!(
    offers.requests(&json!({ "data": { "free_model_daily_requests": { "used": 1, "limit": 50 } } })),
    None,
    "an allowance that states no figure left is none"
  );

  // A provider that declares no second address is asked once and states none.
  let deepseek = DeepSeek.balance().unwrap();

  assert_eq!(deepseek.requests_url(&machine), None);
  assert_eq!(
    deepseek.requests(&json!({ "free_model_daily_requests": { "used": 1, "limit": 2, "remaining": 1 } })),
    None
  );
}

#[test]
fn only_the_providers_that_declare_a_balance_offer_one() {
  let offering: Vec<&str> = harness_providers::providers()
    .into_iter()
    .filter(|one| one.balance().is_some())
    .map(|one| one.name())
    .collect();

  assert_eq!(offering, ["openrouter", "deepseek"]);
}

#[test]
fn a_stand_in_s_address_wins_and_an_endpoint_on_its_origin_is_the_provider_s() {
  let dir = tempfile::tempdir().unwrap();
  let offers = DeepSeek.balance().unwrap();
  let own = Machine::with_env([], dir.path());
  let stood_in = Machine::with_env(
    [("BMO_DEEPSEEK_BALANCE_URL".to_owned(), "http://[::1]:9/user/balance".to_owned())],
    dir.path(),
  );

  assert_eq!(offers.url(&own), "https://api.deepseek.com/user/balance");
  assert_eq!(offers.url(&stood_in), "http://[::1]:9/user/balance");
  assert!(offers.serves("https://api.deepseek.com/anthropic", &own));
  assert!(!offers.serves("https://api.deepseek.com.example/anthropic", &own));
  assert!(!offers.serves("https://openrouter.ai/api", &own));
  assert!(offers.serves("http://[::1]:9/anthropic", &stood_in));
  assert!(!offers.serves("not an address", &own));
}

#[test]
fn each_road_asks_with_its_own_key() {
  let dir = tempfile::tempdir().unwrap();
  let machine = Machine::with_env([], dir.path());
  let key = DeepSeek::key_file(&machine);
  let openrouter = dir.path().join("env").join("openrouter.key");

  assert!(DeepSeek.balance().unwrap().key(&machine).is_err());
  assert!(provider("openrouter").unwrap().balance().unwrap().key(&machine).is_err());

  std::fs::create_dir_all(key.parent().unwrap()).unwrap();
  std::fs::write(&key, "sk-deep\n").unwrap();
  std::fs::set_permissions(&key, std::fs::Permissions::from_mode(0o600)).unwrap();
  std::fs::write(&openrouter, "sk-or\n").unwrap();
  std::fs::set_permissions(&openrouter, std::fs::Permissions::from_mode(0o600)).unwrap();

  assert_eq!(DeepSeek.balance().unwrap().key(&machine), Ok("sk-deep".into()));
  assert_eq!(provider("openrouter").unwrap().balance().unwrap().key(&machine), Ok("sk-or".into()));
}

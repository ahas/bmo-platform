//! Each provider's balance as the server asks it: the provider's own road where the provider can answer on
//! this machine, and every endpoint account whose endpoint stands on the provider's balance origin, with the
//! account's own key file; an endpoint elsewhere and a provider that offers no balance are never asked.

use std::os::unix::fs::PermissionsExt;

use agency_server::usage_windows::ProviderBalances;
use cache_siren::balance::BalanceSource;
use harness_providers::deepseek::DeepSeek;
use harness_providers::{Machine, provider};

use crate::support::scratch::Scratch;

#[tokio::test]
async fn the_road_and_each_endpoint_account_on_the_provider_s_origin_are_asked_with_their_own_keys() {
  let scratch = Scratch::new().await;
  let dir = tempfile::tempdir().unwrap();
  let machine = Machine::with_env(
    [("BMO_DEEPSEEK_BALANCE_URL".to_owned(), "http://[::1]:9/user/balance".to_owned())],
    dir.path(),
  );
  let paid_key = dir.path().join("paid.key");

  std::fs::write(&paid_key, "sk-paid\n").unwrap();
  scratch
    .run(
      r#"INSERT INTO "claude_accounts"
           ("id", "name", "place", "enabled", "credential_kind", "credential_path", "added_at",
            "endpoint_url", "key_file")
         VALUES ('main', 'one', 1, 1, 'directory', '/logins/one', 0, NULL, NULL),
                ('paid', 'two', 2, 0, 'endpoint', '/logins/paid', 0, 'http://[::1]:9/anthropic', $1),
                ('elsewhere', 'three', 3, 1, 'endpoint', '/logins/else', 0, 'http://127.0.0.1:8/anthropic', $1)"#,
      vec![paid_key.to_string_lossy().into_owned().into()],
    )
    .await;

  let balances = ProviderBalances::of(&DeepSeek, &machine, scratch.db()).unwrap();
  let asked = |asks: Vec<cache_siren::balance::BalanceAsk>| {
    asks.into_iter().map(|ask| (ask.account, ask.url, ask.key)).collect::<Vec<_>>()
  };

  assert_eq!(balances.mode(), "deepseek");
  assert_eq!(
    asked(balances.asks().await.unwrap()),
    [("paid".to_owned(), "http://[::1]:9/user/balance".to_owned(), Ok("sk-paid".to_owned()))],
    "the road is not asked while its key file is missing"
  );

  let road_key = DeepSeek::key_file(&machine);

  std::fs::create_dir_all(road_key.parent().unwrap()).unwrap();
  std::fs::write(&road_key, "sk-road").unwrap();
  std::fs::set_permissions(&road_key, std::fs::Permissions::from_mode(0o600)).unwrap();

  let gateway = dir.path().join("env").join("llm-gateway.key");

  std::fs::write(&gateway, "sk-gateway\n").unwrap();
  std::fs::set_permissions(&gateway, std::fs::Permissions::from_mode(0o600)).unwrap();

  let members = dir.path().join("llm-gateway");

  std::fs::create_dir_all(&members).unwrap();
  std::fs::write(
    members.join("members.json"),
    r#"[{"name":"deepseek-flash","road":"deepseek","provider":"deepseek","model":"deepseek-flash","api_base":"https://api.deepseek.com/v1","key_file":"deepseek.key","context_tokens":1048576,"answer_tokens":32768,"thinking":true}]"#,
  )
  .unwrap();

  let asks = asked(balances.asks().await.unwrap());

  assert_eq!(asks.len(), 2);
  assert_eq!(
    asks[0],
    (String::new(), "http://[::1]:9/user/balance".to_owned(), Ok("sk-road".to_owned()))
  );
  assert!(ProviderBalances::of(provider("codex").unwrap(), &machine, scratch.db()).is_none());
}

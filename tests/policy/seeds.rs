use policy::seeds::SEEDS;

#[test]
fn the_account_is_the_machines_login_and_the_repository_seeds_no_model_file() {
  assert_eq!((SEEDS.account.id.as_str(), SEEDS.account.name.as_str()), ("default", "Default account"));
  assert!(SEEDS.catalogue.is_empty());
}

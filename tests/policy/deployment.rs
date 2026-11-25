use policy::deployment::*;

#[test]
fn an_unset_or_blank_variable_leaves_the_neutral_default() {
  let names =
    DEPLOYMENT_NAMES.read(|name| (name == DEPLOYMENT_NAMES.owner_name.variable).then(|| " ".into()));

  assert_eq!(
    names,
    Deployment {
      agent_key: DEPLOYMENT_NAMES.agent_key.default.clone(),
      agent_name: DEPLOYMENT_NAMES.agent_name.default.clone(),
      owner_name: DEPLOYMENT_NAMES.owner_name.default.clone(),
      sender_name: DEPLOYMENT_NAMES.sender_name.default.clone(),
    }
  );
  assert!(
    [&names.agent_key, &names.agent_name, &names.owner_name, &names.sender_name]
      .iter()
      .all(|name| !name.is_empty())
  );
}

#[test]
fn a_set_variable_names_the_deployment_its_own_way() {
  let names =
    DEPLOYMENT_NAMES.read(|name| (name == DEPLOYMENT_NAMES.agent_name.variable).then(|| "Kit".into()));

  assert_eq!(names.agent_name, "Kit");
  assert_eq!(names.owner_name, DEPLOYMENT_NAMES.owner_name.default);
}

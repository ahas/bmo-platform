use agency_server::control::own_llm::*;
use agency_server::control::registry::OwnLlmService;

fn service() -> OwnLlmService {
  OwnLlmService {
    name: "registered",
    label: "Qwen".into(),
    short_label: "Qwen".into(),
    base_url: "http://[::1]:9".into(),
    key_file: "llm-gateway.key".into(),
    registration: "qwen".into(),
  }
}

#[test]
fn the_list_must_name_the_registration_once() {
  let listed = |ids: &[&str]| ids.iter().map(|id| (*id).to_owned()).collect::<Vec<_>>();

  assert_eq!(selectable(&service(), &listed(&["other", "qwen"])), Ok("qwen".to_owned()));
  assert_eq!(
    selectable(&service(), &listed(&["other"])).map_err(|why| why.kind),
    Err("catalogue-changed")
  );
  assert_eq!(
    selectable(&service(), &listed(&["qwen", "qwen"])).map_err(|why| why.kind),
    Err("catalogue-changed")
  );
}

#[test]
fn a_failure_names_the_service_and_its_status() {
  assert_eq!(OwnLlmError::status(503).said(&service()), "Qwen service (503)");
  assert_eq!(OwnLlmError::of("catalogue-changed").said(&service()), "Qwen catalogue-changed");
}

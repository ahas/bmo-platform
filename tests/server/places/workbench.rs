//! A worker's workbench as the job unit reads it, of a running server: its folder, and every item's scratch
//! folder standing under it in item order, a folder named for no item and a stray file left out; a worker
//! whose workbench was never made answers its folder and no scratch.

use policy::job_prompt::JOB_PROMPT;
use serde_json::{Value, json};

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

#[tokio::test(flavor = "multi_thread")]
async fn a_running_server_answers_a_worker_s_workbench_and_its_items_scratches() {
  let scratch = Scratch::new().await;
  let home = tempfile::tempdir().unwrap();
  let data = home.path().join("data");
  let token = "secret";
  let bench = data.join("workbench").join("helper");
  let items = bench.join(&JOB_PROMPT.workbench.scratch);

  for folder in ["12", "3", "notes"] {
    std::fs::create_dir_all(items.join(folder)).unwrap();
  }

  std::fs::write(items.join("7"), "").unwrap();

  let server = standing::stand_server(home.path(), &data, &scratch.address(), token);
  let port = server.port;
  let shown = |path: std::path::PathBuf| path.to_string_lossy().into_owned();

  assert_eq!(
    get(port, token, "/repositories/helper/workbench").await,
    (
      200,
      json!({ "root": shown(bench.clone()), "scratches": [shown(items.join("3")), shown(items.join("12"))] })
    )
  );
  assert_eq!(
    get(port, token, "/repositories/newcomer/workbench").await,
    (200, json!({ "root": shown(data.join("workbench").join("newcomer")), "scratches": [] }))
  );
}

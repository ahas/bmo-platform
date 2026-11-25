//! A supervisor's settings and prompt folder as its row holds them.

use agency_server::conversational::*;

use crate::support::rows::supervisor;
use crate::support::scratch::Scratch;

#[test]
fn a_row_is_read_by_its_scope_and_shape_and_its_scope_gives_the_shape_it_names_none_of() {
  let read = Settings::read(Some("agent"), Some("window")).unwrap();

  assert_eq!(read.scope, Scope::Agent);
  assert_eq!(read.shape, Shape::Window);
  assert_eq!(Settings::read(Some("room"), None).unwrap().shape, Shape::Pieces);
  assert_eq!(Settings::read(Some("agent"), None).unwrap().shape, Shape::Window);
}

#[test]
fn a_row_with_no_scope_the_runner_carries_is_none() {
  assert_eq!(Settings::read(None, None), None);
  assert_eq!(Settings::read(Some("elsewhere"), None), None);
}

#[tokio::test]
async fn a_supervisor_s_prompt_folder_is_the_one_its_row_names_and_none_otherwise() {
  let scratch = Scratch::new().await;

  supervisor(scratch.db(), "pal", &[("prompts", "pal-prompts".into())]).await;
  supervisor(scratch.db(), "added", &[]).await;

  assert_eq!(prompt_folder(scratch.db(), "pal").await.unwrap().as_deref(), Some("pal-prompts"));
  assert_eq!(prompt_folder(scratch.db(), "added").await.unwrap(), None);
  assert_eq!(prompt_folder(scratch.db(), "nobody").await.unwrap(), None);
}

//! The pane claims: none where no consumer registered, and the consumer's own where one did.

use agency_server::hooks::Hooks;
use agency_server::places::claims::{PaneClaim, PaneClaims};

#[tokio::test]
async fn nothing_is_claimed_until_a_consumer_claims_its_panes() {
  let hooks = Hooks::default();

  assert!(hooks.ask::<PaneClaims>(()).await.is_empty());

  hooks.register::<PaneClaims, _, _>(|()| async {
    vec![PaneClaim {
      agent: "slot-1".into(),
      label: "slot 1".into(),
      idle: true,
    }]
  });

  let claimed = hooks.ask::<PaneClaims>(()).await;

  assert_eq!(claimed.len(), 1);
  assert!(claimed[0].idle);
}

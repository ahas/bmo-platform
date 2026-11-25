//! A run on the OpenRouter road given the figures the tap read: its spend filed under the road it came on,
//! and a reply the tap read no usage for left as it was.

use agency_server::door::record::{ModelSpend, Reply, RequestUsage};
use agency_server::job_run::openrouter::observed;
use agency_server::job_run::openrouter_tap::{Observation, Usage};

fn reply() -> Reply {
  Reply {
    mode: "openrouter".into(),
    text: "done".into(),
    ..Reply::default()
  }
}

#[test]
fn the_tap_s_figures_are_the_reply_s_spend_and_its_last_request() {
  let total = Usage {
    input_tokens: 30,
    output_tokens: 6,
    cache_read_tokens: 60,
    cache_write_tokens: 10,
    cost: 0.5,
  };
  let last = Usage {
    input_tokens: 10,
    output_tokens: 1,
    ..Usage::default()
  };
  let seen = Observation {
    upstream: Some("Anthropic".into()),
    total: Some(total),
    last: Some(last),
  };
  let read = observed(reply(), "model-a", &seen);

  assert_eq!(
    read.spend,
    [ModelSpend {
      mode: "openrouter".into(),
      model: "model-a".into(),
      tokens: 106,
      input_tokens: 30,
      output_tokens: 6,
      cache_read_tokens: 60,
      cache_write_tokens: 10,
      cost: 0.5,
    }]
  );
  assert_eq!(
    read.last_request_usage,
    Some(RequestUsage {
      input_tokens: 10,
      cache_read_tokens: 0,
      cache_write_tokens: 0,
      output_tokens: 1,
    })
  );
}

#[test]
fn a_reply_the_tap_read_no_usage_for_keeps_what_dsh_gave() {
  assert_eq!(observed(reply(), "model-a", &Observation::default()), reply());
}

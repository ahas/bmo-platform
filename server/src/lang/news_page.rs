//! The morning news page and the message that hands it to a room (docs/news-brief.md, 「The page」), each
//! line with the `{name}` marks its file names.

use std::collections::BTreeMap;

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NewsPage {
  pub weekdays: [String; 7],
  pub spoken_day: String,
  pub title: String,
  pub news: String,
  pub korea_market: String,
  pub us_market: String,
  pub real_estate: String,
  pub no_close: String,
  pub close_of: String,
  pub chart_label: String,
  pub original: String,
  pub sources: String,
  pub footer: String,
  pub indices: BTreeMap<String, String>,
  pub failed: String,
}

crate::part!(pub NEWS_PAGE: NewsPage = "news-page");

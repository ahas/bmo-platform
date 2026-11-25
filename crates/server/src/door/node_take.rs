//! The bound a program on a node is held to before it takes its turn (docs/work-nodes.md, 「When a node drops」): a
//! program can stand on a node and never come to take its words while the link answers for it as standing, so the
//! time it stands there with the node's link heard is counted, the turn's waiting named as the notice passes and
//! the turn ended as the bound does. A while the link is not heard is the link's, and counting starts afresh after it.
//! A program on a person's PC whose far end could not listen on a port it reaches home through is named at once and
//! held to a bound of its own, since it can never take its turn.

use std::time::Duration;

use policy::fill_marks;
use policy::resident_turn::RESIDENT_TURN;
use tokio::time::Instant;

/// What one look at the bound comes to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Take {
  /// The program is still within its bound, or was already said to stand past its notice.
  Within,
  /// The program has just stood past its notice: why the turn waits, and how long ago the count began.
  Notice { reason: String, stood: Duration },
  /// The program has stood past its bound, and why the turn ends.
  Overdue(String),
  /// The far end listens again on the port it was said not to, so the waiting told of it is cleared.
  Cleared,
}

/// One turn's count of how long its program has stood on a node without taking the turn.
#[derive(Clone, Debug)]
pub struct NodeTake {
  node: String,
  notice: Duration,
  bound: Duration,
  listen_bound: Duration,
  // Since when the program has stood with the node's link heard; none while it is not.
  from: Option<Instant>,
  noticed: bool,
  // Since when the far end has said it could not listen on a port, and whether that was told.
  refused_from: Option<Instant>,
  refused_told: bool,
}

impl NodeTake {
  pub fn new(node: &str, notice: Duration, bound: Duration, listen_bound: Duration) -> Self {
    Self {
      node: node.to_owned(),
      notice,
      bound,
      listen_bound,
      from: None,
      noticed: false,
      refused_from: None,
      refused_told: false,
    }
  }

  /// The count for a program on the node named, held to the resident turn's policy.
  pub fn of_policy(node: &str) -> Self {
    Self::new(
      node,
      Duration::from_millis(RESIDENT_TURN.node_take_notice_ms),
      Duration::from_millis(RESIDENT_TURN.node_take_bound_ms),
      Duration::from_millis(RESIDENT_TURN.node_listen_bound_ms),
    )
  }

  pub fn node(&self) -> &str {
    &self.node
  }

  /// Whether a waiting was told, so it is cleared once the words are taken.
  pub fn noticed(&self) -> bool {
    self.noticed || self.refused_told
  }

  /// One look, with whether the node's link is heard at the moment given and the first port its far end could not
  /// listen on, with why, where there is one.
  pub fn look(&mut self, heard: bool, refused: Option<(u16, &str)>, now: Instant) -> Take {
    if !heard {
      self.from = None;
      self.refused_from = None;

      return Take::Within;
    }

    match refused {
      Some((port, why)) => {
        let stood = now.saturating_duration_since(*self.refused_from.get_or_insert(now));
        let said = |words: &str| {
          fill_marks(words, &[("node", self.node.as_str()), ("port", &port.to_string()), ("why", why)])
        };

        if stood >= self.listen_bound {
          return Take::Overdue(said(&RESIDENT_TURN.node_listen_cut));
        }

        if !self.refused_told {
          self.refused_told = true;

          return Take::Notice {
            reason: said(&RESIDENT_TURN.node_listen_waiting),
            stood,
          };
        }
      }
      None => {
        self.refused_from = None;

        if std::mem::take(&mut self.refused_told) {
          return Take::Cleared;
        }
      }
    }

    let stood = now.saturating_duration_since(*self.from.get_or_insert(now));
    let said = |words: &str| {
      fill_marks(words, &[("node", self.node.as_str()), ("seconds", &stood.as_secs().to_string())])
    };

    if stood >= self.bound {
      return Take::Overdue(said(&RESIDENT_TURN.node_take_cut));
    }

    if stood >= self.notice && !self.noticed {
      self.noticed = true;

      return Take::Notice {
        reason: said(&RESIDENT_TURN.node_take_waiting),
        stood,
      };
    }

    Take::Within
  }
}

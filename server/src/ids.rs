//! A user's id on the wire (docs/boundary.md, 「Users and senders」): a string a caller hands on as it stands.
//! The service keeps sumida's whole-number id; it is written out as its digits and read from a string of
//! them or from the number itself.

use serde::de::{self, Deserializer, Visitor};

/// A user's id as the service answers it.
pub fn shown(user: i32) -> String {
  user.to_string()
}

/// A user's id read off a path segment.
pub fn parsed(user: &str) -> Option<i32> {
  user.trim().parse().ok()
}

struct UserVisitor;

impl Visitor<'_> for UserVisitor {
  type Value = i32;

  fn expecting(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
    out.write_str("a user's id, as a string of digits or a number")
  }

  fn visit_str<E: de::Error>(self, text: &str) -> Result<i32, E> {
    parsed(text).ok_or_else(|| E::custom(format!("`{text}` is no user's id")))
  }

  fn visit_i64<E: de::Error>(self, number: i64) -> Result<i32, E> {
    i32::try_from(number).map_err(|_| E::custom(format!("{number} is no user's id")))
  }

  fn visit_u64<E: de::Error>(self, number: u64) -> Result<i32, E> {
    i32::try_from(number).map_err(|_| E::custom(format!("{number} is no user's id")))
  }
}

/// A user's id read from a string or a number.
pub fn user<'de, D: Deserializer<'de>>(reader: D) -> Result<i32, D::Error> {
  reader.deserialize_any(UserVisitor)
}

/// A user's id that may be missing or null, read from a string or a number.
pub fn user_or_none<'de, D: Deserializer<'de>>(reader: D) -> Result<Option<i32>, D::Error> {
  struct Maybe;

  impl<'de> Visitor<'de> for Maybe {
    type Value = Option<i32>;

    fn expecting(&self, out: &mut std::fmt::Formatter) -> std::fmt::Result {
      out.write_str("a user's id, or null")
    }

    fn visit_none<E: de::Error>(self) -> Result<Option<i32>, E> {
      Ok(None)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Option<i32>, E> {
      Ok(None)
    }

    fn visit_some<D: Deserializer<'de>>(self, reader: D) -> Result<Option<i32>, D::Error> {
      user(reader).map(Some)
    }
  }

  reader.deserialize_option(Maybe)
}

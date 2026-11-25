//! A JSON value read and written back as the TypeScript run log writes it: its keys in their order, and a
//! name written twice keeping its place and its last value.

use agency_server::run_log::json::Json;

#[test]
fn an_object_is_written_back_in_the_order_it_came() {
  let text = r#"{"z":1,"a":{"y":[true,null,"é\n"],"b":2.5},"m":-3}"#;

  assert_eq!(Json::parse(text).unwrap().stringify(), text);
}

#[test]
fn a_name_written_twice_keeps_its_place_and_its_last_value() {
  assert_eq!(Json::parse(r#"{"a":1,"b":2,"a":3}"#).unwrap().stringify(), r#"{"a":3,"b":2}"#);
}

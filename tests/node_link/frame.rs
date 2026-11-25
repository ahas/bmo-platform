//! A frame of the link reads back as it was written.

use node_link::frame::*;

#[test]
fn a_frame_reads_back_as_written() {
  let frame = Frame::Append {
    pull: 2,
    path: "a/b.jsonl".into(),
    offset: 9,
    data: encode(b"{\"x\":1}\n"),
    list: Some(3),
  };
  let back = Frame::read(&frame.line()).expect("reads");

  assert_eq!(back, frame);
}

#[test]
fn a_watch_from_an_end_that_numbers_no_list_reads_as_holding_nothing() {
  assert_eq!(
    Frame::read(r#"{"t":"watch","pulls":[]}"#),
    Some(Frame::Watch {
      pulls: vec![],
      held: vec![],
      list: None
    })
  );
}

use super::*;
use cognix_core::command::ExitCode;

fn block_with_output(command: &str, stdout: &str, stderr: &str, code: i32) -> Block {
    let mut b = Block::new(BlockId::new(), BlockIndex(0), command);
    b.set_output(BlockOutput::from_bytes(stdout.as_bytes(), stderr.as_bytes()));
    b.finish(ExitCode::from(code), 5);
    b
}

#[test]
fn block_id_from_counter_uses_session_and_counter() {
    let id = BlockId::from_counter("sess", 7);
    assert_eq!(id.as_str(), "sess-7");
}

#[test]
fn block_id_new_is_distinct_and_manual() {
    let a = BlockId::new();
    let b = BlockId::new();
    assert_ne!(a, b);
    assert!(a.as_str().starts_with("manual-"));
}

#[test]
fn block_index_arithmetic() {
    assert_eq!(BlockIndex::zero().next(), BlockIndex(1));
    assert_eq!(usize::from(BlockIndex::from(4)), 4);
    assert_eq!(BlockIndex::from(0).to_string(), "0");
}

#[test]
fn block_index_range_as_iter() {
    let got: Vec<usize> = BlockIndex::range_as_iter(BlockIndex(1)..BlockIndex(4))
        .map(usize::from)
        .collect();
    assert_eq!(got, vec![1, 2, 3]);
}

#[test]
fn block_output_from_bytes_keeps_streams_apart() {
    let out = BlockOutput::from_bytes(b"a\nb\n", b"err\n");
    assert_eq!(out.len(), 3);
    assert_eq!(out.lines()[0].stream, Stream::Stdout);
    assert_eq!(out.lines()[2].stream, Stream::Stderr);
    assert_eq!(out.lines()[2].text, "err");
}

#[test]
fn block_output_trailing_newline_does_not_make_empty_line() {
    let out = BlockOutput::from_bytes(b"only\n", b"");
    assert_eq!(out.len(), 1);
    assert_eq!(out.lines()[0].text, "only");
}

#[test]
fn block_output_handles_crlf_and_empty() {
    assert_eq!(BlockOutput::from_bytes(b"x\r\ny\r\n", b"").len(), 2);
    assert!(BlockOutput::from_bytes(b"", b"").is_empty());
}

#[test]
fn block_output_replaces_invalid_utf8_rather_than_dropping() {
    let out = BlockOutput::from_bytes(&[b'a', 0xff, b'b'], b"");
    assert_eq!(out.len(), 1);
    assert!(out.lines()[0].text.contains('a'));
}

#[test]
fn block_output_to_plain_text_round_trips() {
    let out = BlockOutput::from_bytes(b"one\ntwo\n", b"three\n");
    assert_eq!(out.to_plain_text(), "one\ntwo\nthree\n");
}

#[test]
fn empty_output_is_empty_not_a_single_blank_line() {
    assert!(BlockOutput::from_bytes(b"\n", b"\n").is_empty());
}

#[test]
fn block_starts_running() {
    let b = Block::new(BlockId::new(), BlockIndex(0), "ls");
    assert!(b.state().is_running());
    assert!(!b.is_successful());
    assert_eq!(b.state().exit_code(), None);
}

#[test]
fn block_finish_records_exit_code_and_duration() {
    let mut b = Block::new(BlockId::new(), BlockIndex(0), "ls");
    b.finish(ExitCode::from(0), 42);
    assert!(b.is_successful());
    assert_eq!(b.duration_ms(), Some(42));
    assert!(!b.state().is_running());
}

#[test]
fn block_exit_codes_follow_cognix_success_semantics() {
    for code in [0, 130, 141] {
        let mut b = block_with_output("x", "", "", code);
        b.set_output(BlockOutput::new());
        assert!(b.is_successful(), "expected {code} to count as success");
    }

    let mut b = block_with_output("x", "", "", 1);
    b.set_output(BlockOutput::new());
    assert!(!b.is_successful());
}

#[test]
fn block_interrupted_is_not_successful() {
    let mut b = Block::new(BlockId::new(), BlockIndex(0), "x");
    b.interrupt();
    assert_eq!(b.state(), BlockState::Interrupted);
    assert!(!b.is_successful());
}

#[test]
fn block_toggle_bookmark_round_trips() {
    let mut b = Block::new(BlockId::new(), BlockIndex(0), "x");
    assert!(!b.is_bookmarked());
    assert!(b.toggle_bookmark());
    assert!(b.is_bookmarked());
    assert!(!b.toggle_bookmark());
    assert!(!b.is_bookmarked());
}

#[test]
fn block_has_stderr_detects_error_output() {
    let b = block_with_output("x", "ok\n", "boom\n", 1);
    assert!(b.has_stderr());

    let b = block_with_output("x", "ok\n", "", 0);
    assert!(!b.has_stderr());
}

#[test]
fn block_summary_marks_bookmarks_and_failures() {
    let mut b = block_with_output("cargo build", "", "", 0);
    b.set_bookmarked(true);
    assert!(b.summary().starts_with('*'));
    assert!(b.summary().contains("cargo build"));
    assert!(b.summary().contains("[ok]"));

    let b = block_with_output("false", "", "", 1);
    assert!(b.summary().contains("exit 1"));
}

#[test]
fn block_output_iterates_over_lines() {
    let out = BlockOutput::from_bytes(b"a\nb\n", b"");
    let collected: Vec<&OutputLine> = (&out).into_iter().collect();
    assert_eq!(collected.len(), 2);
    assert_eq!(collected[0].text, "a");
}

#[test]
fn block_output_collects_from_iterator() {
    let out: BlockOutput = [OutputLine::stdout("x"), OutputLine::stderr("y")]
        .into_iter()
        .collect();
    assert_eq!(out.len(), 2);
}

#[test]
fn block_display_of_stream_is_lowercase() {
    assert_eq!(Stream::Stdout.to_string(), "stdout");
    assert_eq!(Stream::Stderr.to_string(), "stderr");
}

#[test]
fn block_id_parses_from_str() {
    use std::str::FromStr;
    let id = BlockId::from_str("abc").unwrap();
    assert_eq!(id.as_str(), "abc");
}
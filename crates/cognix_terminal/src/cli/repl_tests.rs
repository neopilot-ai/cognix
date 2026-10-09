use super::*;
use crate::block::{BlockIndex, BlockOutput};
use cognix_core::command::ExitCode;
use parking_lot::Mutex as PlMutex;
use std::sync::Arc;

/// A write sink that appends into a buffer the test can read afterwards.
#[derive(Clone)]
struct Capture(Arc<PlMutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn sink() -> (Capture, Arc<PlMutex<Vec<u8>>>) {
    let buf = Arc::new(PlMutex::new(Vec::new()));
    (Capture(buf.clone()), buf)
}

fn text(buf: &Arc<PlMutex<Vec<u8>>>) -> String {
    String::from_utf8_lossy(&buf.lock()).into_owned()
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

/// A REPL seeded with three blocks, returning it plus its output buffer.
fn seeded_repl() -> (Repl, Arc<PlMutex<Vec<u8>>>) {
    let session = Session::new("test");
    for (cmd, out, err, code) in [
        ("git status", "On branch main\n", "", 0),
        ("cargo build", "Compiling\n", "error: boom\n", 1),
        ("ls", "README.md\n", "", 0),
    ] {
        let i = session.terminal.begin_block(cmd);
        session
            .terminal
            .with_block(i, |b| {
                b.set_output(BlockOutput::from_bytes(out.as_bytes(), err.as_bytes()));
                b.finish(ExitCode::from(code), 1);
            })
            .unwrap();
    }
    let (writer, buf) = sink();
    (Repl::new(session).with_output(Box::new(writer)), buf)
}

#[test]
fn help_lists_the_directives() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":help"));
    let out = text(&buf);
    assert!(out.contains(":blocks"));
    assert!(out.contains(":bookmark"));
    assert!(out.contains(":share"));
}

#[test]
fn blocks_lists_every_block() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":blocks"));
    let out = text(&buf);
    assert!(out.contains("git status"));
    assert!(out.contains("cargo build"));
    assert!(out.contains("ls"));
}

#[test]
fn blocks_can_list_only_bookmarks() {
    let (mut repl, buf) = seeded_repl();
    repl.session()
        .terminal
        .toggle_bookmark(BlockIndex(1))
        .unwrap();
    runtime().block_on(repl.handle_line(":blocks -b"));
    let out = text(&buf);
    assert!(out.contains("cargo build"));
    assert!(!out.contains("git status"));
}

#[test]
fn show_prints_block_output() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":show 0"));
    assert!(text(&buf).contains("On branch main"));
}

#[test]
fn show_marks_stderr_lines() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":show 1"));
    let out = text(&buf);
    assert!(out.contains("! error: boom"), "got {out}");
}

#[test]
fn show_reports_unknown_blocks() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":show 99"));
    assert!(text(&buf).contains("error"));
}

#[test]
fn current_prints_the_block_at_the_cursor() {
    let (mut repl, buf) = seeded_repl();
    repl.session().navigator.lock().seek(BlockIndex(2));
    runtime().block_on(repl.handle_line(":current"));
    assert!(text(&buf).contains("README.md"));
}

#[test]
fn search_reports_matching_lines() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":search branch"));
    let out = text(&buf);
    assert!(out.contains("On branch main"));
    assert!(out.contains("1 match"));
}

#[test]
fn filter_moves_the_cursor_to_the_first_match() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":filter README"));
    assert!(text(&buf).contains("cursor -> block 2"));
    assert_eq!(
        repl.session().navigator.lock().current(),
        Some(BlockIndex(2))
    );
}

#[test]
fn search_does_not_move_the_cursor() {
    let (mut repl, _buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":search README"));
    assert_eq!(repl.session().navigator.lock().current(), None);
}

#[test]
fn search_reports_when_nothing_matches() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":search zzzz"));
    assert!(text(&buf).contains("error"));
}

#[test]
fn bookmark_toggles_and_confirms() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":bookmark 0"));
    assert!(text(&buf).contains("bookmarked block 0"));
    assert!(repl
        .session()
        .terminal
        .get_by_index(BlockIndex(0))
        .unwrap()
        .is_bookmarked());
}

#[test]
fn bookmark_twice_removes_the_bookmark() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":bookmark 0"));
    runtime().block_on(repl.handle_line(":bookmark 0"));
    assert!(text(&buf).contains("unbookmarked block 0"));
}

#[test]
fn bookmark_on_empty_session_does_not_panic() {
    let (writer, _buf) = sink();
    let mut repl = Repl::new(Session::new("t")).with_output(Box::new(writer));
    runtime().block_on(repl.handle_line(":bookmark"));
    assert!(repl.session().terminal.is_empty());
}

#[test]
fn share_emits_markdown_for_the_current_block() {
    let (mut repl, buf) = seeded_repl();
    repl.session().navigator.lock().seek(BlockIndex(1));
    runtime().block_on(repl.handle_line(":share"));
    let out = text(&buf);
    assert!(out.contains("cargo build"));
    assert!(out.contains("error: boom"));
}

#[test]
fn share_emits_json_when_asked() {
    let (mut repl, buf) = seeded_repl();
    repl.session().navigator.lock().seek(BlockIndex(0));
    runtime().block_on(repl.handle_line(":share json"));
    let out = text(&buf);
    let value: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert_eq!(value["command"], "git status");
}

#[test]
fn share_all_covers_every_block() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":share -a"));
    let out = text(&buf);
    assert!(out.contains("git status"));
    assert!(out.contains("ls"));
}

#[test]
fn nav_moves_and_reports_the_cursor() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":nav last"));
    assert!(text(&buf).contains("cursor -> block 2"));
    assert_eq!(
        repl.session().navigator.lock().current(),
        Some(BlockIndex(2))
    );
}

#[test]
fn nav_prev_moves_backwards() {
    let (mut repl, _buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":nav last"));
    runtime().block_on(repl.handle_line(":nav prev"));
    assert_eq!(
        repl.session().navigator.lock().current(),
        Some(BlockIndex(1))
    );
}

#[test]
fn running_a_command_creates_a_block() {
    let (writer, _buf) = sink();
    let mut repl = Repl::new(Session::new("t")).with_output(Box::new(writer));
    runtime().block_on(repl.handle_line("echo hi"));
    assert_eq!(repl.session().terminal.len(), 1);
    assert_eq!(
        repl.session().navigator.lock().current(),
        Some(BlockIndex(0))
    );
}

#[test]
fn running_a_command_prints_its_output() {
    let (writer, buf) = sink();
    let mut repl = Repl::new(Session::new("t")).with_output(Box::new(writer));
    runtime().block_on(repl.handle_line("echo hello-there"));
    assert!(text(&buf).contains("hello-there"));
}

#[test]
fn shell_metacharacters_are_refused() {
    let (writer, _buf) = sink();
    let mut repl = Repl::new(Session::new("t")).with_output(Box::new(writer));
    runtime().block_on(repl.handle_line("ls | wc"));
    // Rejected before a block is created, so nothing is captured.
    assert!(repl.session().terminal.is_empty());
}

#[test]
fn exit_stops_the_loop() {
    let (writer, _buf) = sink();
    let mut repl = Repl::new(Session::new("t")).with_output(Box::new(writer));
    assert!(!runtime().block_on(repl.handle_line(":exit")));
}

#[test]
fn blank_lines_are_ignored() {
    let (writer, _buf) = sink();
    let mut repl = Repl::new(Session::new("t")).with_output(Box::new(writer));
    assert!(runtime().block_on(repl.handle_line("   ")));
    assert!(repl.session().terminal.is_empty());
}

#[test]
fn unknown_directives_report_an_error() {
    let (writer, buf) = sink();
    let mut repl = Repl::new(Session::new("t")).with_output(Box::new(writer));
    runtime().block_on(repl.handle_line(":bogus"));
    assert!(text(&buf).contains("unknown directive"));
}

#[test]
fn session_directive_prints_the_session_id() {
    let (mut repl, buf) = seeded_repl();
    runtime().block_on(repl.handle_line(":session"));
    assert!(text(&buf).contains("test"));
}
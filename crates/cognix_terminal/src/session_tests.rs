use super::*;
use cognix_core::command::ExitCode;
use crate::runner::RunError;

fn session_with(commands: &[&str]) -> TerminalSession {
    let s = TerminalSession::new("test");
    for (i, c) in commands.iter().enumerate() {
        let index = s.begin_block(*c);
        assert_eq!(index, BlockIndex(i));
    }
    s
}

#[test]
fn session_indices_are_sequential_from_zero() {
    let s = session_with(&["a", "b", "c"]);
    assert_eq!(s.len(), 3);
    assert_eq!(s.get_by_index(BlockIndex(0)).unwrap().command(), "a");
    assert_eq!(s.get_by_index(BlockIndex(2)).unwrap().command(), "c");
}

#[test]
fn session_block_ids_share_the_session_prefix() {
    let s = session_with(&["a", "b"]);
    assert_eq!(s.get_by_index(BlockIndex(0)).unwrap().id().as_str(), "test-0");
    assert_eq!(s.get_by_index(BlockIndex(1)).unwrap().id().as_str(), "test-1");
}

#[test]
fn session_lookup_by_id_finds_the_block() {
    let s = session_with(&["a", "b"]);
    let id = s.get_by_index(BlockIndex(1)).unwrap().id().clone();
    assert_eq!(s.get_by_id(&id).unwrap().command(), "b");
}

#[test]
fn session_unknown_lookups_report_the_reference() {
    let s = session_with(&["a"]);

    let err = s.get_by_index(BlockIndex(9)).unwrap_err();
    assert!(matches!(err, SessionError::UnknownBlockIndex(BlockIndex(9))));

    let err = s.get_by_id(&BlockId::from("nope".to_owned())).unwrap_err();
    assert!(matches!(err, SessionError::UnknownBlockId(_)));

    let err = TerminalSession::new("empty").last_index().unwrap_err();
    assert!(matches!(err, SessionError::Empty));
}

#[test]
fn session_last_index_tracks_the_end() {
    let s = session_with(&["a", "b", "c"]);
    assert_eq!(s.last_index().unwrap(), BlockIndex(2));
}

#[test]
fn session_bookmarks_track_toggles() {
    let s = session_with(&["a", "b", "c"]);
    assert!(s.toggle_bookmark(BlockIndex(1)).unwrap());
    assert!(s.toggle_bookmark(BlockIndex(2)).unwrap());
    assert_eq!(s.bookmarked_indices(), vec![BlockIndex(1), BlockIndex(2)]);
    assert!(!s.toggle_bookmark(BlockIndex(1)).unwrap());
    assert_eq!(s.bookmarked_indices(), vec![BlockIndex(2)]);
}

#[test]
fn session_running_indices_finds_unfinished_blocks() {
    let s = session_with(&["a", "b"]);
    assert_eq!(s.running_indices(), vec![BlockIndex(0), BlockIndex(1)]);

    s.with_block(BlockIndex(0), |b| b.finish(ExitCode::from(0), 1))
        .unwrap();
    assert_eq!(s.running_indices(), vec![BlockIndex(1)]);
}

#[test]
fn session_resolve_accepts_index_or_id() {
    let s = session_with(&["a", "b"]);
    assert_eq!(
        s.resolve(&BlockRef::from(BlockIndex(1))).unwrap().command(),
        "b"
    );

    let id = s.get_by_index(BlockIndex(0)).unwrap().id().clone();
    assert_eq!(s.resolve(&BlockRef::from(id)).unwrap().command(), "a");
}

#[test]
fn block_ref_parses_integers_as_indices() {
    use std::str::FromStr;
    assert_eq!(
        BlockRef::from_str("3").unwrap(),
        BlockRef::Index(BlockIndex(3))
    );
    assert_eq!(
        BlockRef::from_str("abc").unwrap(),
        BlockRef::Id(BlockId::from("abc".to_owned()))
    );
}

#[test]
fn navigator_moves_within_bounds() {
    let mut n = Navigator::new();
    assert_eq!(n.current(), None);

    n.next(3);
    assert_eq!(n.current(), Some(BlockIndex(0)), "clamps at start");

    n.next(3);
    n.next(3);
    assert_eq!(n.current(), Some(BlockIndex(2)), "clamps at end");

    n.next(3);
    assert_eq!(n.current(), Some(BlockIndex(2)));

    n.prev(3);
    assert_eq!(n.current(), Some(BlockIndex(1)));
}

#[test]
fn navigator_first_and_last() {
    let mut n = Navigator::new();
    n.seek(BlockIndex(5));
    n.first();
    assert_eq!(n.current(), Some(BlockIndex(0)));

    n.last(4);
    assert_eq!(n.current(), Some(BlockIndex(3)));

    n.last(0);
    assert_eq!(n.current(), None);
}

#[test]
fn navigator_on_empty_session_has_no_position() {
    let mut n = Navigator::new();
    n.next(0);
    assert_eq!(n.current(), None);
}

#[test]
fn session_block_at_follows_the_navigator() {
    let session = Session::new("test");
    for c in ["a", "b"] {
        session.terminal.begin_block(c);
    }

    session.navigator.lock().seek(BlockIndex(1));
    let block = session
        .terminal
        .block_at(&session.navigator.lock())
        .unwrap();
    assert_eq!(block.command(), "b");
}

#[test]
fn session_finalize_running_interrupts_unfinished_blocks() {
    let session = Session::new("test");
    session.terminal.begin_block("a");
    session.terminal.begin_block("b");
    session
        .terminal
        .with_block(BlockIndex(0), |b| b.finish(ExitCode::from(0), 1))
        .unwrap();

    session.finalize_running();

    assert_eq!(session.terminal.running_indices(), Vec::<BlockIndex>::new());
    assert!(session
        .terminal
        .get_by_index(BlockIndex(0))
        .unwrap()
        .is_successful());
}

#[test]
fn session_run_captures_a_real_command_into_a_block() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let session = runtime.block_on(async { Session::new("test") });
    let block = runtime
        .block_on(session.run("echo hello"))
        .expect("run should succeed");

    assert_eq!(block.command(), "echo hello");
    assert!(block.is_successful());
    assert_eq!(block.output().to_plain_text().trim(), "hello");
    assert_eq!(session.terminal.len(), 1);
}

#[test]
fn session_run_records_nonzero_exit_codes() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let session = Session::new("test");
    let block = runtime.block_on(session.run("false")).unwrap();
    assert!(!block.is_successful());
}

#[test]
fn session_run_rejects_empty_commands() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let session = Session::new("test");
    let err = runtime.block_on(session.run("   ")).unwrap_err();
    assert!(matches!(err, RunError::EmptyCommand));
}

#[test]
fn session_run_reports_missing_programs() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let session = Session::new("test");
    let err = runtime
        .block_on(session.run("definitely-not-a-real-binary-xyz"))
        .unwrap_err();
    assert!(matches!(err, RunError::NotFound(_)), "got {err:?}");
}
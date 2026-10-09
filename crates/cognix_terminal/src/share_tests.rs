use super::*;
use crate::block::{BlockIndex, BlockOutput};
use cognix_core::command::ExitCode;


fn seeded() -> TerminalSession {
    let s = TerminalSession::new("t");
    let a = s.begin_block("git status");
    s.with_block(a, |b| {
        b.set_output(BlockOutput::from_bytes(b"On branch main\n", b""));
        b.set_cwd("/repo");
        b.finish(ExitCode::from(0), 10);
    })
    .unwrap();

    let b = s.begin_block("cargo build");
    s.with_block(b, |b| {
        b.set_output(BlockOutput::from_bytes(b"Compiling\n", b"error: boom\n"));
        b.finish(ExitCode::from(1), 20);
    })
    .unwrap();

    s
}

#[test]
fn markdown_share_wraps_command_and_output() {
    let s = seeded();
    let md = share_block(&s.get_by_index(BlockIndex(0)).unwrap(), ShareFormat::Markdown).unwrap();
    assert!(md.contains("## `git status`"));
    assert!(md.contains("```console"));
    assert!(md.contains("On branch main"));
    assert!(md.contains("```"));
}

#[test]
fn markdown_share_escapes_backticks_in_the_command() {
    let s = TerminalSession::new("t");
    let i = s.begin_block("echo `hi`");
    s.with_block(i, |b| b.finish(ExitCode::from(0), 1)).unwrap();
    let md = share_block(&s.get_by_index(i).unwrap(), ShareFormat::Markdown).unwrap();
    assert!(md.contains("echo 'hi'"), "got {md}");
}

#[test]
fn plain_text_share_is_output_only() {
    let s = seeded();
    let txt = share_block(&s.get_by_index(BlockIndex(0)).unwrap(), ShareFormat::PlainText).unwrap();
    assert_eq!(txt, "On branch main\n");
    assert!(!txt.contains("git status"));
}

#[test]
fn json_share_is_valid_and_round_trips() {
    let s = seeded();
    let json = share_block(&s.get_by_index(BlockIndex(0)).unwrap(), ShareFormat::Json).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();

    assert_eq!(value["command"], "git status");
    assert_eq!(value["cwd"], "/repo");
    assert_eq!(value["successful"], true);
    assert_eq!(value["output"][0]["stream"], "stdout");
    assert_eq!(value["output"][0]["text"], "On branch main");
}

#[test]
fn json_share_records_stderr_stream() {
    let s = seeded();
    let json = share_block(&s.get_by_index(BlockIndex(1)).unwrap(), ShareFormat::Json).unwrap();
    let value: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(value["output"][1]["stream"], "stderr");
    assert_eq!(value["successful"], false);
}

#[test]
fn share_blocks_joins_multiple_documents() {
    let s = seeded();
    let blocks = s.blocks();
    let out = share_blocks(&blocks, ShareFormat::Markdown).unwrap();
    assert!(out.contains("git status"));
    assert!(out.contains("cargo build"));
}

#[test]
fn share_bookmarked_exports_only_bookmarked_blocks() {
    let s = seeded();
    s.toggle_bookmark(BlockIndex(1)).unwrap();
    let out = share_bookmarked(&s, ShareFormat::Markdown).unwrap();
    assert!(out.contains("cargo build"));
    assert!(!out.contains("git status"));
}

#[test]
fn share_bookmarked_is_empty_when_no_bookmarks() {
    let s = seeded();
    let out = share_bookmarked(&s, ShareFormat::Markdown).unwrap();
    assert!(out.is_empty());
}

#[test]
fn share_format_parses_aliases() {
    assert_eq!(ShareFormat::parse("md"), Some(ShareFormat::Markdown));
    assert_eq!(ShareFormat::parse("JSON"), Some(ShareFormat::Json));
    assert_eq!(ShareFormat::parse("txt"), Some(ShareFormat::PlainText));
    assert_eq!(ShareFormat::parse("nope"), None);
}

#[test]
fn share_format_defaults_to_markdown() {
    assert_eq!(ShareFormat::default(), ShareFormat::Markdown);
}
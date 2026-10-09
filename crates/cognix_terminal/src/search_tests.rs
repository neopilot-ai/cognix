use super::*;
use crate::block::BlockOutput;
use cognix_core::command::ExitCode;

fn seeded() -> TerminalSession {
    let s = TerminalSession::new("t");
    let a = s.begin_block("git status");
    s.with_block(a, |b| {
        b.set_output(BlockOutput::from_bytes(
            b"On branch main\nnothing to commit\n",
            b"",
        ));
        b.finish(ExitCode::from(0), 1);
    })
    .unwrap();

    let b = s.begin_block("cargo build");
    s.with_block(b, |b| {
        b.set_output(BlockOutput::from_bytes(
            b"Compiling\n",
            b"error: unresolved import\n",
        ));
        b.finish(ExitCode::from(1), 2);
    })
    .unwrap();

    let c = s.begin_block("ls -la");
    s.with_block(c, |b| {
        b.set_output(BlockOutput::from_bytes(b"README.md\n", b""));
        b.finish(ExitCode::from(0), 3);
    })
    .unwrap();

    s
}

#[test]
fn substring_search_matches_output_and_command() {
    let s = seeded();
    let hits = search(&s, &SearchQuery::new("branch")).unwrap();
    assert_eq!(hits, vec![BlockIndex(0)]);
}

#[test]
fn search_is_case_insensitive_by_default() {
    let s = seeded();
    assert_eq!(
        search(&s, &SearchQuery::new("COMMIT")).unwrap(),
        vec![BlockIndex(0)]
    );
}

#[test]
fn case_sensitive_search_can_be_requested() {
    let s = seeded();
    let q = SearchQuery::new("COMMIT").case_sensitive(true);
    assert!(matches!(search(&s, &q), Err(SearchError::NoMatches)));
}

#[test]
fn search_scope_command_only() {
    let s = seeded();
    let q = SearchQuery::new("cargo").with_scope(SearchScope::Command);
    let hits = search(&s, &q).unwrap();
    assert_eq!(hits, vec![BlockIndex(1)]);
}

#[test]
fn search_scope_output_only() {
    let s = seeded();
    let q = SearchQuery::new("README").with_scope(SearchScope::Output);
    assert_eq!(search(&s, &q).unwrap(), vec![BlockIndex(2)]);
}

#[test]
fn search_can_filter_to_successful_blocks() {
    let s = seeded();
    let q = SearchQuery::new("e").successful_only(true);
    let hits = search(&s, &q).unwrap();
    assert!(!hits.contains(&BlockIndex(1)), "build block failed");
}

#[test]
fn search_can_filter_to_bookmarked_blocks() {
    let s = seeded();
    s.toggle_bookmark(BlockIndex(2)).unwrap();
    let q = SearchQuery::new("e").bookmarked_only(true);
    let hits = search(&s, &q).unwrap();
    assert_eq!(hits, vec![BlockIndex(2)]);
}

#[test]
fn regex_search_supports_alternation() {
    let s = seeded();
    let q = SearchQuery::regex("branch|cargo");
    let hits = search(&s, &q).unwrap();
    assert_eq!(hits, vec![BlockIndex(0), BlockIndex(1)]);
}

#[test]
fn invalid_regex_is_reported() {
    let s = seeded();
    let q = SearchQuery::regex("(unclosed");
    assert!(matches!(search(&s, &q), Err(SearchError::Regex(_))));
}

#[test]
fn search_with_no_hits_reports_no_matches() {
    let s = seeded();
    assert!(matches!(
        search(&s, &SearchQuery::new("zzzz-not-present")),
        Err(SearchError::NoMatches)
    ));
}

#[test]
fn detailed_search_reports_the_matching_lines() {
    let s = seeded();
    let matches = search_detailed(&s, &SearchQuery::new("branch")).unwrap();
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].block_index, BlockIndex(0));
    assert_eq!(matches[0].line_number, Some(0));
    assert_eq!(matches[0].source, MatchSource::Output);
    assert_eq!(matches[0].text, "On branch main");
}

#[test]
fn command_matches_have_no_line_number() {
    let s = seeded();
    let matches = search_detailed(&s, &SearchQuery::new("cargo")).unwrap();
    let cmd_match = matches.iter().find(|m| m.source == MatchSource::Command).unwrap();
    assert_eq!(cmd_match.line_number, None);
    assert_eq!(cmd_match.block_index, BlockIndex(1));
}

#[test]
fn search_finds_stderr_too() {
    let s = seeded();
    let hits = search(&s, &SearchQuery::new("unresolved")).unwrap();
    assert_eq!(hits, vec![BlockIndex(1)]);
}

#[test]
fn query_compiles_with_case_insensitivity_by_default() {
    let re = SearchQuery::new("AbC").compile().unwrap();
    assert!(re.is_match("xxabcxx"));
}
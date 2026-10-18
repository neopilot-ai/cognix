use super::*;
use crate::block::BlockIndex;

#[test]
fn plain_lines_are_not_directives() {
    assert_eq!(parse_directive("cargo build").unwrap(), None);
    assert_eq!(parse_directive("ls -la").unwrap(), None);
    assert_eq!(parse_directive("").unwrap(), None);
}

#[test]
fn help_and_exit_parse() {
    assert_eq!(parse_directive(":help").unwrap(), Some(Directive::Help));
    assert_eq!(parse_directive(":exit").unwrap(), Some(Directive::Exit));
    assert_eq!(parse_directive(":quit").unwrap(), Some(Directive::Exit));
}

#[test]
fn blocks_directive_parses_bookmark_flag() {
    assert_eq!(
        parse_directive(":blocks").unwrap(),
        Some(Directive::Blocks {
            bookmarked_only: false
        })
    );
    assert_eq!(
        parse_directive(":blocks -b").unwrap(),
        Some(Directive::Blocks {
            bookmarked_only: true
        })
    );
}

#[test]
fn show_parses_index_and_id() {
    assert_eq!(
        parse_directive(":show 3").unwrap(),
        Some(Directive::Show(BlockRef::from(BlockIndex(3))))
    );
    assert_eq!(
        parse_directive(":show abc").unwrap(),
        Some(Directive::Show(BlockRef::from(crate::block::BlockId::from(
            "abc".to_owned()
        ))))
    );
}

#[test]
fn search_collects_multi_word_query() {
    let Some(Directive::Search(d)) = parse_directive(":search to be").unwrap() else {
        panic!("expected a search directive");
    };
    assert_eq!(d.pattern(), "to be");
    assert_eq!(d.kind, QueryKind::Substring);
}

#[test]
fn search_requires_a_query() {
    assert!(parse_directive(":search").is_err());
    assert!(parse_directive(":search   ").is_err());
}

#[test]
fn search_flags_map_to_options() {
    let Some(Directive::Search(d)) = parse_directive(":search foo -r -s --ok -b -o").unwrap()
    else {
        panic!("expected a search directive");
    };
    assert_eq!(d.kind, QueryKind::Regex);
    assert!(d.case_sensitive);
    assert!(d.successful_only);
    assert!(d.bookmarked_only);
    assert_eq!(d.scope, SearchScope::Output);
}

#[test]
fn search_supports_command_only_scope() {
    let Some(Directive::Search(d)) = parse_directive(":search foo -c").unwrap() else {
        panic!("expected a search directive");
    };
    assert_eq!(d.scope, SearchScope::Command);
}

#[test]
fn filter_is_the_interactive_variant_of_search() {
    let Some(Directive::Search(d)) = parse_directive(":filter foo").unwrap() else {
        panic!("expected a search directive");
    };
    assert!(d.interactive());

    let Some(Directive::Search(d)) = parse_directive(":search foo").unwrap() else {
        panic!("expected a search directive");
    };
    assert!(!d.interactive());
}

#[test]
fn bookmark_targets_current_when_no_ref_given() {
    assert_eq!(
        parse_directive(":bookmark").unwrap(),
        Some(Directive::Bookmark {
            target: BookmarkTarget::Current,
            set: None
        })
    );
}

#[test]
fn bookmark_accepts_a_ref_and_explicit_set() {
    assert_eq!(
        parse_directive(":bookmark 2 -on").unwrap(),
        Some(Directive::Bookmark {
            target: BookmarkTarget::Ref(BlockRef::from(BlockIndex(2))),
            set: Some(true)
        })
    );
    assert_eq!(
        parse_directive(":bm 2 -off").unwrap(),
        Some(Directive::Bookmark {
            target: BookmarkTarget::Ref(BlockRef::from(BlockIndex(2))),
            set: Some(false)
        })
    );
}

#[test]
fn share_parses_format_and_ref() {
    assert_eq!(
        parse_directive(":share 1 json").unwrap(),
        Some(Directive::Share {
            target: Some(BlockRef::from(BlockIndex(1))),
            format: ShareFormat::Json,
            all: false
        })
    );
}

#[test]
fn share_all_sets_the_all_flag() {
    let Some(Directive::Share { all, .. }) = parse_directive(":share -a").unwrap() else {
        panic!("expected a share directive");
    };
    assert!(all);
}

#[test]
fn copy_parses_optional_ref() {
    assert_eq!(
        parse_directive(":copy").unwrap(),
        Some(Directive::Copy { target: None })
    );
    assert_eq!(
        parse_directive(":copy 4").unwrap(),
        Some(Directive::Copy {
            target: Some(BlockRef::from(BlockIndex(4)))
        })
    );
}

#[test]
fn nav_parses_directions_and_targets() {
    assert_eq!(
        parse_directive(":nav next").unwrap(),
        Some(Directive::Nav(NavDirective::Next))
    );
    assert_eq!(
        parse_directive(":nav prev").unwrap(),
        Some(Directive::Nav(NavDirective::Prev))
    );
    assert_eq!(
        parse_directive(":nav first").unwrap(),
        Some(Directive::Nav(NavDirective::First))
    );
    assert_eq!(
        parse_directive(":nav last").unwrap(),
        Some(Directive::Nav(NavDirective::Last))
    );
    assert_eq!(
        parse_directive(":nav 7").unwrap(),
        Some(Directive::Nav(NavDirective::Go(BlockRef::from(
            BlockIndex(7)
        ))))
    );
}

#[test]
fn nav_rejects_unknown_targets() {
    assert!(parse_directive(":nav sideways").is_err());
}

#[test]
fn unknown_directives_are_reported() {
    assert_eq!(
        parse_directive(":nonsense").unwrap(),
        Some(Directive::Unknown("nonsense".to_owned()))
    );
}

#[test]
fn session_directive_parses() {
    assert_eq!(parse_directive(":session").unwrap(), Some(Directive::Session));
}
#[test]
fn complete_parses_the_line_to_complete() {
    assert_eq!(
        parse_directive(":complete git st").unwrap(),
        Some(Directive::Complete {
            line: "git st".to_owned()
        })
    );
}

#[test]
fn complete_with_no_arguments_completes_an_empty_line() {
    assert_eq!(
        parse_directive(":complete").unwrap(),
        Some(Directive::Complete {
            line: String::new()
        })
    );
}

#[test]
fn highlight_defaults_to_the_cursor_and_autodetect() {
    assert_eq!(
        parse_directive(":highlight").unwrap(),
        Some(Directive::Highlight {
            target: None,
            language: None,
            annotated: false
        })
    );
}

#[test]
fn highlight_parses_a_block_ref_and_language() {
    assert_eq!(
        parse_directive(":highlight 2 rust").unwrap(),
        Some(Directive::Highlight {
            target: Some(BlockRef::from(BlockIndex(2))),
            language: Some("rust".to_owned()),
            annotated: false
        })
    );
}

#[test]
fn highlight_supports_annotated_output() {
    assert_eq!(
        parse_directive(":highlight 0 python --annotated").unwrap(),
        Some(Directive::Highlight {
            target: Some(BlockRef::from(BlockIndex(0))),
            language: Some("python".to_owned()),
            annotated: true
        })
    );
}

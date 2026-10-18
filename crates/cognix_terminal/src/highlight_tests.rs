use super::*;

const RUST: &str = "fn main() {\n    // greet the world\n    let x: i32 = 42;\n    println!(\"hi {}\", x);\n}\n";

#[test]
fn detects_rust_from_a_file_name() {
    assert_eq!(detect_language("main.rs"), Some("rust"));
}

#[test]
fn detects_other_common_languages() {
    assert_eq!(detect_language("a.py"), Some("python"));
    assert_eq!(detect_language("a.go"), Some("go"));
    assert_eq!(detect_language("a.ts"), Some("typescript"));
    assert_eq!(detect_language("a.json"), Some("json"));
}

#[test]
fn undetectable_extensions_report_none() {
    assert_eq!(detect_language("noextension"), None);
    assert_eq!(detect_language("weird.zzzz"), None);
}

#[test]
fn highlight_path_reports_undetectable_files() {
    let mut h = Highlighter::new();
    let err = highlight_path(&mut h, "mystery.zzzz", "body").unwrap_err();
    assert!(matches!(err, HighlightError::UndetectableLanguage(_)));
}

#[test]
fn highlighting_rust_produces_spans() {
    let mut h = Highlighter::new();
    let spans = highlight(&mut h, "rust", RUST).unwrap();
    assert!(!spans.is_empty(), "expected spans for rust source");
    assert!(spans.iter().all(|s| s.end <= RUST.len()));
}

#[test]
fn spans_are_sorted_and_non_overlapping() {
    let mut h = Highlighter::new();
    let spans = highlight(&mut h, "rust", RUST).unwrap();

    for pair in spans.windows(2) {
        assert!(
            pair[0].start <= pair[1].start,
            "spans must be sorted by start"
        );
        assert!(pair[0].end <= pair[1].start, "spans must not overlap");
    }
}

#[test]
fn spans_cover_recognisable_source_text() {
    let mut h = Highlighter::new();
    let spans = highlight(&mut h, "rust", RUST).unwrap();

    let comment = spans
        .iter()
        .find(|s| s.kind == TokenKind::Comment)
        .and_then(|s| s.text(RUST));
    assert_eq!(comment, Some("// greet the world"));
}

#[test]
fn empty_source_yields_no_spans() {
    let mut h = Highlighter::new();
    assert!(highlight(&mut h, "rust", "").unwrap().is_empty());
}

#[test]
fn to_ansi_emits_escapes_and_keeps_all_text() {
    let mut h = Highlighter::new();
    let spans = highlight(&mut h, "rust", RUST).unwrap();
    let rendered = to_ansi(&spans, RUST);

    assert!(rendered.contains("\x1b["), "expected ANSI escapes");
    // Stripping escapes must recover the original source exactly.
    let stripped: String = strip_ansi(&rendered);
    assert_eq!(stripped, RUST);
}

#[test]
fn to_annotated_tags_spans_and_keeps_all_text() {
    let mut h = Highlighter::new();
    let spans = highlight(&mut h, "rust", RUST).unwrap();
    let annotated = to_annotated(&spans, RUST);

    assert!(annotated.contains("[comment]"), "got {annotated}");
    assert!(annotated.contains("[keyword]") || annotated.contains("[function]"));
}

#[test]
fn rendering_handles_unhighlighted_source() {
    let spans: Vec<HighlightSpan> = Vec::new();
    assert_eq!(to_ansi(&spans, "plain text"), "plain text");
    assert_eq!(to_annotated(&spans, "plain text"), "plain text");
}

#[test]
fn spans_out_of_bounds_are_skipped_not_panicking() {
    let bad = HighlightSpan {
        start: 5,
        end: 500,
        kind: TokenKind::Other,
        capture: "x".to_owned(),
    };
    let out = to_ansi(&[bad], "abc");
    assert!(out.contains("abc") || out.is_empty());
}

#[test]
fn capture_names_map_onto_token_kinds() {
    assert_eq!(TokenKind::from_capture("comment"), TokenKind::Comment);
    assert_eq!(TokenKind::from_capture("keyword"), TokenKind::Keyword);
    assert_eq!(TokenKind::from_capture("function.builtin"), TokenKind::Function);
    assert_eq!(TokenKind::from_capture("string"), TokenKind::String);
    assert_eq!(TokenKind::from_capture("punctuation.special"), TokenKind::Punctuation);
    assert_eq!(TokenKind::from_capture("totally-unknown"), TokenKind::Other);
}

#[test]
fn token_kinds_have_stable_labels() {
    assert_eq!(TokenKind::Comment.label(), "comment");
    assert_eq!(TokenKind::Keyword.label(), "keyword");
    assert_eq!(TokenKind::Other.label(), "other");
}

#[test]
fn highlighting_multiple_languages_works() {
    let mut h = Highlighter::new();
    assert!(highlight(&mut h, "python", "def f():\n    return 1\n").is_ok());
    assert!(highlight(&mut h, "json", "{\"a\": 1}").is_ok());
    assert!(highlight(&mut h, "bash", "echo hi").is_ok());
}

/// Removes ANSI SGR escape sequences so rendered text can be compared to the source.
fn strip_ansi(input: &str) -> String {
    let mut out = String::new();
    let mut chars = input.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}
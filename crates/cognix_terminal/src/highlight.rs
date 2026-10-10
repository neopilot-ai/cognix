//! Syntax highlighting for captured block output.
//!
//! Warp's `syntax_tree` depends on `warp_editor` and `languages` (170 files) which cascade into
//! `asset_cache` and a git dependency. This highlights standalone source text instead, using
//! `arborium`'s bundled grammars, so a block's output can be rendered as ANSI without the editor
//! stack.

use arborium::Highlighter;

/// The semantic class of a highlighted span.
///
/// Grammar capture names are many and language-specific (`keyword`, `storageclass`,
/// `function.builtin`, `punctuation.special`, ...). This folds them into a small set so callers
/// can style consistently without knowing each grammar's vocabulary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TokenKind {
    Comment,
    Keyword,
    Function,
    Type,
    String,
    Number,
    Constant,
    Variable,
    Operator,
    Punctuation,
    Tag,
    Property,
    Attribute,
    Other,
}

impl TokenKind {
    /// Maps a tree-sitter highlight capture name onto a [`TokenKind`].
    ///
    /// Matching is on the part before the first `.` and then a set of aliases, because grammars
    /// disagree on naming for the same visual category.
    pub fn from_capture(capture: &str) -> Self {
        let head = capture.split('.').next().unwrap_or(capture);
        match head {
            "comment" => TokenKind::Comment,
            "keyword" | "conditional" | "repeat" | "storageclass" => TokenKind::Keyword,
            "function" | "method" | "macro" => TokenKind::Function,
            "type" | "class" | "struct" | "enum" | "interface" | "namespace" => TokenKind::Type,
            "string" | "character" | "text" => TokenKind::String,
            "number" | "float" | "integer" | "boolean" | "constant.builtin" => TokenKind::Number,
            "constant" => TokenKind::Constant,
            "variable" | "parameter" | "property" => TokenKind::Variable,
            "operator" => TokenKind::Operator,
            "punctuation" | "punctuation.special" | "punctuation.bracket" => TokenKind::Punctuation,
            "tag" => TokenKind::Tag,
            "attribute" | "label" | "embedded" => TokenKind::Attribute,
            _ => TokenKind::Other,
        }
    }

    /// A short, stable label for CLI output.
    pub fn label(&self) -> &'static str {
        match self {
            TokenKind::Comment => "comment",
            TokenKind::Keyword => "keyword",
            TokenKind::Function => "function",
            TokenKind::Type => "type",
            TokenKind::String => "string",
            TokenKind::Number => "number",
            TokenKind::Constant => "const",
            TokenKind::Variable => "variable",
            TokenKind::Operator => "operator",
            TokenKind::Punctuation => "punct",
            TokenKind::Tag => "tag",
            TokenKind::Property => "property",
            TokenKind::Attribute => "attribute",
            TokenKind::Other => "other",
        }
    }
}

/// A highlighted byte range in the source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HighlightSpan {
    pub start: usize,
    pub end: usize,
    pub kind: TokenKind,
    /// The grammar capture name, kept for callers that want finer styling.
    pub capture: String,
}

impl HighlightSpan {
    /// The source text this span covers, or `None` if the range is out of bounds.
    pub fn text<'a>(&self, source: &'a str) -> Option<&'a str> {
        source.get(self.start..self.end)
    }
}

/// Errors from highlighting.
#[derive(Debug, thiserror::Error)]
pub enum HighlightError {
    #[error("no grammar for language `{0}`")]
    UnknownLanguage(String),
    #[error("could not detect a language for `{0}`")]
    UndetectableLanguage(String),
    #[error("highlighting failed: {0}")]
    Engine(String),
}

/// Detects the language for `path` from its extension.
pub fn detect_language(path: &str) -> Option<&'static str> {
    arborium::detect_language(path)
}

/// Highlights `source` as `language`.
///
/// Returns spans sorted by start offset with overlaps removed, so callers can walk the source
/// linearly. Grammar failures are surfaced rather than silently returning no spans, because a
/// silently unhighlighted block is indistinguishable from a plain-text one.
pub fn highlight(
    highlighter: &mut Highlighter,
    language: &str,
    source: &str,
) -> Result<Vec<HighlightSpan>, HighlightError> {
    let spans = highlighter
        .highlight_spans(language, source)
        .map_err(|e| HighlightError::Engine(e.to_string()))?;

    let mut converted: Vec<HighlightSpan> = spans
        .into_iter()
        .map(|s| HighlightSpan {
            start: s.start as usize,
            end: s.end as usize,
            kind: TokenKind::from_capture(&s.capture),
            capture: s.capture,
        })
        .filter(|s| s.start < s.end && s.end <= source.len())
        .collect();

    converted.sort_by_key(|s| (s.start, std::cmp::Reverse(s.end)));

    // Later captures are higher priority in tree-sitter, so when ranges overlap keep the first
    // (highest-priority) one and drop anything already covered.
    let mut resolved: Vec<HighlightSpan> = Vec::with_capacity(converted.len());
    for span in converted {
        if resolved.last().is_some_and(|last| span.start < last.end) {
            continue;
        }
        resolved.push(span);
    }
    Ok(resolved)
}

/// Highlights `source`, detecting the language from `path`.
pub fn highlight_path(
    highlighter: &mut Highlighter,
    path: &str,
    source: &str,
) -> Result<Vec<HighlightSpan>, HighlightError> {
    let language = detect_language(path)
        .ok_or_else(|| HighlightError::UndetectableLanguage(path.to_owned()))?;
    highlight(highlighter, language, source)
}

/// Renders `source` with ANSI colour, one line at a time.
///
/// Grammars frequently emit spans that cross line boundaries; the remaining source between spans
/// is emitted unstyled so no text is ever dropped.
pub fn to_ansi(spans: &[HighlightSpan], source: &str) -> String {
    let mut out = String::with_capacity(source.len() * 2);
    let mut cursor = 0usize;

    for span in spans {
        let Some(text) = span.text(source) else { continue };
        if span.start < cursor {
            continue;
        }
        out.push_str(&source[cursor..span.start]);
        out.push_str(colour_for(span.kind));
        out.push_str(text);
        out.push_str("\x1b[0m");
        cursor = span.end;
    }
    if cursor < source.len() {
        out.push_str(&source[cursor..]);
    }
    out
}

/// The ANSI escape prefix for a token kind.
fn colour_for(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Comment => "\x1b[90m",
        TokenKind::Keyword => "\x1b[35m",
        TokenKind::Function => "\x1b[34m",
        TokenKind::Type => "\x1b[36m",
        TokenKind::String => "\x1b[32m",
        TokenKind::Number => "\x1b[33m",
        TokenKind::Constant => "\x1b[36m",
        TokenKind::Variable => "\x1b[0m",
        TokenKind::Operator => "\x1b[31m",
        TokenKind::Punctuation => "\x1b[37m",
        TokenKind::Tag => "\x1b[34m",
        TokenKind::Property => "\x1b[0m",
        TokenKind::Attribute => "\x1b[33m",
        TokenKind::Other => "\x1b[0m",
    }
}

/// Renders spans as annotated text, e.g. `let` tagged `keyword`.
///
/// Useful when writing highlighted output to a file or diffing it, where ANSI escapes would be noise.
pub fn to_annotated(spans: &[HighlightSpan], source: &str) -> String {
    let mut out = String::new();
    let mut cursor = 0usize;
    for span in spans {
        let Some(text) = span.text(source) else { continue };
        if span.start < cursor {
            continue;
        }
        out.push_str(&source[cursor..span.start]);
        out.push_str(text);
        out.push_str(&format!("[{}]", span.kind.label()));
        cursor = span.end;
    }
    if cursor < source.len() {
        out.push_str(&source[cursor..]);
    }
    out
}

#[cfg(test)]
#[path = "highlight_tests.rs"]
mod tests;
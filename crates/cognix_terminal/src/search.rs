//! Searching and filtering across a session's blocks.

use crate::block::{Block, BlockIndex};
use crate::session::TerminalSession;

/// Which part of a block a query should be matched against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SearchScope {
    /// Only the command text.
    Command,
    /// Only the captured output.
    Output,
    /// Both command and output. This is the default.
    #[default]
    All,
}

/// How a query's pattern is interpreted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QueryKind {
    /// A plain substring, matched case-insensitively.
    #[default]
    Substring,
    /// A regular expression.
    Regex,
}

/// A search over a session's blocks.
#[derive(Clone, Debug)]
pub struct SearchQuery {
    pattern: String,
    kind: QueryKind,
    scope: SearchScope,
    case_sensitive: bool,
    /// Only consider blocks that finished successfully.
    successful_only: bool,
    /// Only consider bookmarked blocks.
    bookmarked_only: bool,
}

impl SearchQuery {
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            kind: QueryKind::Substring,
            scope: SearchScope::All,
            case_sensitive: false,
            successful_only: false,
            bookmarked_only: false,
        }
    }

    pub fn regex(pattern: impl Into<String>) -> Self {
        Self {
            kind: QueryKind::Regex,
            ..Self::new(pattern)
        }
    }

    pub fn with_scope(mut self, scope: SearchScope) -> Self {
        self.scope = scope;
        self
    }

    pub fn with_kind(mut self, kind: QueryKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn case_sensitive(mut self, case_sensitive: bool) -> Self {
        self.case_sensitive = case_sensitive;
        self
    }

    pub fn successful_only(mut self, successful_only: bool) -> Self {
        self.successful_only = successful_only;
        self
    }

    pub fn bookmarked_only(mut self, bookmarked_only: bool) -> Self {
        self.bookmarked_only = bookmarked_only;
        self
    }

    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    /// Matches a single block against this query, returning the lines that matched.
    ///
    /// Returning the matching lines (not just a bool) is what lets the CLI show *why* a block
    /// matched, which is the difference between a useful filter and an opaque one.
    pub fn matches(&self, block: &Block) -> Result<Vec<Match>, SearchError> {
        if self.successful_only && !block.is_successful() {
            return Ok(Vec::new());
        }
        if self.bookmarked_only && !block.is_bookmarked() {
            return Ok(Vec::new());
        }

        let needle = if self.case_sensitive {
            self.pattern.clone()
        } else {
            self.pattern.to_lowercase()
        };

        let mut matches = Vec::new();

        if matches!(self.scope, SearchScope::Command | SearchScope::All)
            && self.haystack(block.command()).contains(&needle)
        {
            matches.push(Match {
                block_index: block.index(),
                line_number: None,
                text: block.command().to_owned(),
                source: MatchSource::Command,
            });
        }

        if matches!(self.scope, SearchScope::Output | SearchScope::All) {
            for (i, line) in block.output().lines().iter().enumerate() {
                if self.haystack(&line.text).contains(&needle) {
                    matches.push(Match {
                        block_index: block.index(),
                        line_number: Some(i),
                        text: line.text.clone(),
                        source: MatchSource::Output,
                    });
                }
            }
        }

        Ok(matches)
    }

    fn haystack(&self, text: &str) -> String {
        if self.case_sensitive {
            text.to_owned()
        } else {
            text.to_lowercase()
        }
    }

    /// Compiles the query's pattern into a regex, for `QueryKind::Regex`.
    pub fn compile(&self) -> Result<regex::Regex, SearchError> {
        let mut builder = regex::RegexBuilder::new(&self.pattern);
        builder.case_insensitive(!self.case_sensitive);
        builder.build().map_err(|e| SearchError::Regex(e.to_string()))
    }

    /// Runs the query against a block using the compiled regex path.
    pub fn matches_regex(&self, block: &Block, re: &regex::Regex) -> Vec<Match> {
        if self.successful_only && !block.is_successful() {
            return Vec::new();
        }
        if self.bookmarked_only && !block.is_bookmarked() {
            return Vec::new();
        }
        let mut matches = Vec::new();
        if matches!(self.scope, SearchScope::Command | SearchScope::All) && re.is_match(block.command())
        {
            matches.push(Match {
                block_index: block.index(),
                line_number: None,
                text: block.command().to_owned(),
                source: MatchSource::Command,
            });
        }
        if matches!(self.scope, SearchScope::Output | SearchScope::All) {
            for (i, line) in block.output().lines().iter().enumerate() {
                if re.is_match(&line.text) {
                    matches.push(Match {
                        block_index: block.index(),
                        line_number: Some(i),
                        text: line.text.clone(),
                        source: MatchSource::Output,
                    });
                }
            }
        }
        matches
    }
}

/// Where a match came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatchSource {
    Command,
    Output,
}

/// A single matching line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Match {
    pub block_index: BlockIndex,
    /// 0-based line number within the output, or `None` when the command text matched.
    pub line_number: Option<usize>,
    pub text: String,
    pub source: MatchSource,
}

/// Errors from building or running a search.
#[derive(Debug, thiserror::Error)]
pub enum SearchError {
    #[error("invalid regular expression: {0}")]
    Regex(String),
    #[error("no blocks matched")]
    NoMatches,
}

/// The blocks in `session` that match `query`, in session order.
///
/// This is the entry point for both `search` and `filter` in the CLI; they differ only in
/// presentation.
pub fn search(session: &TerminalSession, query: &SearchQuery) -> Result<Vec<BlockIndex>, SearchError> {
    let blocks = session.blocks();
    let regex = if query.kind == QueryKind::Regex {
        Some(query.compile()?)
    } else {
        None
    };

    let mut out = Vec::new();
    for block in &blocks {
        let matches = match &regex {
            Some(re) => query.matches_regex(block, re),
            None => query.matches(block)?,
        };
        if !matches.is_empty() {
            out.push(block.index());
        }
    }

    if out.is_empty() {
        return Err(SearchError::NoMatches);
    }
    Ok(out)
}

/// The matches in `session` for `query`, with detail.
pub fn search_detailed(
    session: &TerminalSession,
    query: &SearchQuery,
) -> Result<Vec<Match>, SearchError> {
    let blocks = session.blocks();
    let regex = if query.kind == QueryKind::Regex {
        Some(query.compile()?)
    } else {
        None
    };

    let mut out = Vec::new();
    for block in &blocks {
        match &regex {
            Some(re) => out.extend(query.matches_regex(block, re)),
            None => out.extend(query.matches(block)?),
        }
    }

    if out.is_empty() {
        return Err(SearchError::NoMatches);
    }
    Ok(out)
}

#[cfg(test)]
#[path = "search_tests.rs"]
mod tests;
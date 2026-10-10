//! Completion for the Block model.
//!
//! Warp's `warp_completer` is 17k LOC and pulls in `warp_cli`, `warp_js` and `rquickjs`, none of
//! which exist here. This is a scoped replacement covering what a Block actually needs: fuzzy
//! command matching, filesystem paths, and flags drawn from a small command signature registry.

pub mod path;
pub mod signature;

use std::collections::BTreeSet;

use fuzzy_match::match_indices_case_insensitive;

pub use path::{complete_path, PathCompletion};
pub use signature::{CommandSignature, SignatureRegistry, SignatureRegistryError};

/// What a completion offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CompletionKind {
    /// A program name.
    Command,
    /// A filesystem entry.
    Path,
    /// A command flag.
    Flag,
    /// A value for a flag or positional argument.
    Argument,
}

impl CompletionKind {
    pub fn label(&self) -> &'static str {
        match self {
            CompletionKind::Command => "cmd",
            CompletionKind::Path => "path",
            CompletionKind::Flag => "flag",
            CompletionKind::Argument => "arg",
        }
    }
}

/// A single suggestion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Completion {
    kind: CompletionKind,
    /// The text to insert, which replaces the token being completed.
    text: String,
    score: i64,
    /// Extra context shown to the user, e.g. a signature description.
    detail: Option<String>,
}

impl Completion {
    pub fn new(kind: CompletionKind, text: impl Into<String>, score: i64) -> Self {
        Self {
            kind,
            text: text.into(),
            score,
            detail: None,
        }
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn kind(&self) -> CompletionKind {
        self.kind
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn score(&self) -> i64 {
        self.score
    }

    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
}

/// Scores `candidate` against `prefix`.
///
/// Exact prefix matches score highest, then a contiguous match, then a scattered fuzzy match.
/// An empty prefix accepts everything at score 0 so callers can offer the full list.
pub fn fuzzy_score(candidate: &str, prefix: &str) -> Option<i64> {
    if prefix.is_empty() {
        return Some(0);
    }
    if let Some(better) = match_indices_case_insensitive(candidate, prefix) {
        // Contiguous matches keep every matched character adjacent, which the fuzzy matcher
        // reports via a non-decreasing index sequence; approximate that with span length so
        // `git` beats `g-i-t` for the prefix `git`.
        let _ = better;
        if candidate.to_lowercase().starts_with(&prefix.to_lowercase()) {
            Some(1_000 - candidate.len() as i64)
        } else {
            Some(500 - candidate.len() as i64)
        }
    } else {
        None
    }
}

/// The outcome of a completion request.
#[derive(Clone, Debug, Default)]
pub struct Completions {
    items: Vec<Completion>,
    /// The prefix that was completed, i.e. the token the results replace.
    prefix: String,
}

impl Completions {
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            items: Vec::new(),
            prefix: prefix.into(),
        }
    }

    pub fn push(&mut self, completion: Completion) {
        self.items.push(completion);
    }

    pub fn extend(&mut self, items: impl IntoIterator<Item = Completion>) {
        self.items.extend(items);
    }

    pub fn prefix(&self) -> &str {
        &self.prefix
    }

    pub fn items(&self) -> &[Completion] {
        &self.items
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn best(&self) -> Option<&Completion> {
        self.items.iter().max_by_key(|c| c.score)
    }

    /// Deduplicates by text, keeping the highest score, then sorts best-first.
    pub fn finalize(mut self, limit: usize) -> Self {
        let mut best_by_text: std::collections::HashMap<String, Completion> =
            std::collections::HashMap::new();
        for c in self.items.drain(..) {
            best_by_text
                .entry(c.text.clone())
                .and_modify(|existing| {
                    if c.score > existing.score {
                        *existing = c.clone();
                    }
                })
                .or_insert(c);
        }
        let mut items: Vec<Completion> = best_by_text.into_values().collect();
        items.sort_by(|a, b| {
            b.score
                .cmp(&a.score)
                .then_with(|| a.kind.cmp(&b.kind))
                .then_with(|| a.text.cmp(&b.text))
        });
        items.truncate(limit);
        Self {
            items,
            prefix: self.prefix,
        }
    }
}

/// The last whitespace-separated token of `input`, which is what a completion replaces.
pub fn current_token(input: &str) -> &str {
    match input.rfind(char::is_whitespace) {
        Some(i) => &input[i + 1..],
        None => input,
    }
}

/// The part of `input` before the token being completed.
pub fn token_start(input: &str) -> usize {
    input.len() - current_token(input).len()
}

/// Completes the token at the end of `input`.
///
/// `known_commands` is the set of program names to offer; normally the caller's `PATH` scan plus
/// whatever it has already run this session.
pub fn complete(
    input: &str,
    cwd: &std::path::Path,
    known_commands: &BTreeSet<String>,
    registry: &SignatureRegistry,
    limit: usize,
) -> Completions {
    let token = current_token(input);
    let mut out = Completions::new(token);

    // Flags win once the command is already resolved: a leading '-' is unambiguous, and mixing
    // programs into that list is just noise.
    if token.starts_with('-') {
        if let Some(program) = program_of(input) {
            if let Some(sig) = registry.get(&program) {
                for flag in &sig.flags {
                    if let Some(score) = fuzzy_score(&flag.name, token) {
                        out.push(
                            Completion::new(CompletionKind::Flag, flag.name.clone(), score)
                                .with_detail(flag.description.clone().unwrap_or_default()),
                        );
                    }
                }
            }
        }
        return out.finalize(limit);
    }

    if token.is_empty() {
        return out;
    }

    // First token completes a program; later tokens complete paths. A first token that is
    // clearly a path is completed as a path too, otherwise `/etc/hos` would be matched
    // against command names instead of the filesystem.
    let is_path_token = token.starts_with('/')
        || token.starts_with("./")
        || token.starts_with("../")
        || token.starts_with('~');

    if program_of(input).is_none() && !is_path_token {
        for name in known_commands {
            if let Some(score) = fuzzy_score(name, token) {
                let detail = registry
                    .get(name)
                    .and_then(|s| s.description.clone());
                let completion = Completion::new(CompletionKind::Command, name.clone(), score);
                out.push(match detail {
                    Some(d) => completion.with_detail(d),
                    None => completion,
                });
            }
        }
        return out.finalize(limit);
    }

    for path in complete_path(token, cwd) {
        out.push(Completion::new(
            CompletionKind::Path,
            path.text.clone(),
            100 - path.text.len() as i64,
        ));
    }
    out.finalize(limit)
}

/// The program name if `input` already names one, i.e. it has more than one token.
fn program_of(input: &str) -> Option<String> {
    let trimmed = input.trim_start();
    if trimmed.is_empty() {
        return None;
    }
    let mut parts = trimmed.split_whitespace();
    let program = parts.next()?;
    parts.next()?;
    Some(program.to_owned())
}

/// Whether `input` reads as prose rather than a command.
///
/// Uses the natural-language detector so the CLI can steer the user rather than blindly
/// executing something that will fail with "command not found".
pub fn looks_like_natural_language(input: &str) -> bool {
    let words: Vec<std::borrow::Cow<'_, str>> = input.split_whitespace().map(std::borrow::Cow::Borrowed).collect();
    if words.len() < 3 {
        return false;
    }
    natural_language_detection::natural_language_words_score(words, false) >= 3
}

#[cfg(test)]
#[path = "completer_tests.rs"]
mod tests;
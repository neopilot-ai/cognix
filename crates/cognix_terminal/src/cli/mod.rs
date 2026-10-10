//! Command-line interface mode.
//!
//! Any line beginning with `:` is a Cognix directive; everything else is run as a shell command
//! and captured into a new [`Block`]. The `:` prefix keeps built-ins unambiguous against real
//! commands (`search` is also a perfectly good program name).

pub mod repl;

use clap::{Parser, Subcommand, ValueEnum};

use crate::block::BlockIndex;
use crate::search::{QueryKind, SearchScope};
use crate::session::BlockRef;
use crate::share::ShareFormat;

pub use repl::{run_repl, Repl};

/// The Cognix terminal.
#[derive(Debug, Parser)]
#[command(name = "cognix", version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

/// Non-interactive subcommands.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Run a single command, print its output, and exit.
    Run {
        /// The command to run.
        #[arg(required = true, trailing_var_arg = true)]
        command: Vec<String>,
    },

    /// Start the interactive shell.
    Repl {
        /// Run this command first, before entering the REPL.
        #[arg(short, long)]
        exec: Option<String>,
    },

    /// Search the persisted session history.
    Search {
        /// The query.
        query: String,
        #[arg(short, long)]
        regex: bool,
        #[arg(short, long)]
        case_sensitive: bool,
    },
}

/// The scope of a search, as accepted on the command line.
#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum ScopeArg {
    /// Search command and output.
    All,
    /// Search only command text.
    Command,
    /// Search only output.
    Output,
}

impl From<ScopeArg> for SearchScope {
    fn from(s: ScopeArg) -> Self {
        match s {
            ScopeArg::All => SearchScope::All,
            ScopeArg::Command => SearchScope::Command,
            ScopeArg::Output => SearchScope::Output,
        }
    }
}

impl From<ScopeArg> for QueryKind {
    fn from(s: ScopeArg) -> Self {
        let _ = s;
        QueryKind::Substring
    }
}

/// A parsed `:directive`.
#[derive(Debug, PartialEq, Eq)]
pub enum Directive {
    Help,
    Exit,
    /// List blocks.
    Blocks {
        bookmarked_only: bool,
    },
    /// Show a block's output.
    Show(BlockRef),
    /// Show the current block.
    Current,
    Search(SearchDirective),
    /// Toggle or set a bookmark.
    Bookmark {
        target: BookmarkTarget,
        set: Option<bool>,
    },
    /// Share a block.
    Share {
        target: Option<BlockRef>,
        format: ShareFormat,
        all: bool,
    },
    /// Copy a block to the system clipboard.
    Copy {
        target: Option<BlockRef>,
    },
    /// Move the cursor.
    Nav(NavDirective),
    /// Print the session id.
    Session,
    /// Complete the token at the end of a command line.
    Complete {
        /// The line to complete.
        line: String,
    },
    /// Highlight a block's output as source code.
    Highlight {
        /// The block to highlight.
        target: Option<BlockRef>,
        /// Language name, e.g. `rust`. Detected from the command if omitted.
        language: Option<String>,
        /// Emit `[kind]` annotations instead of ANSI colour.
        annotated: bool,
    },
    /// Echo an unknown directive's text, for error reporting.
    Unknown(String),
}

/// Search options as supplied on the command line.
#[derive(Debug, PartialEq, Eq)]
pub struct SearchDirective {
    pub pattern: String,
    pub kind: QueryKind,
    pub scope: SearchScope,
    pub case_sensitive: bool,
    pub successful_only: bool,
    pub bookmarked_only: bool,
    /// Jump to the first match rather than just listing.
    interactive: bool,
}

impl SearchDirective {
    pub fn pattern(&self) -> &str {
        &self.pattern
    }

    pub fn interactive(&self) -> bool {
        self.interactive
    }
}

/// Which block a bookmark directive refers to.
#[derive(Debug, PartialEq, Eq)]
pub enum BookmarkTarget {
    Current,
    Ref(BlockRef),
}

/// Where to move the cursor.
#[derive(Debug, PartialEq, Eq)]
pub enum NavDirective {
    Next,
    Prev,
    First,
    Last,
    Go(BlockRef),
}

/// Parses a REPL line into a [`Directive`].
///
/// Returns `Ok(None)` for lines that are not directives and should be run as shell commands.
pub fn parse_directive(line: &str) -> Result<Option<Directive>, String> {
    let trimmed = line.trim();
    if !trimmed.starts_with(':') {
        return Ok(None);
    }

    let body = &trimmed[1..];
    let (head, rest) = match body.find(char::is_whitespace) {
        Some(i) => (&body[..i], body[i..].trim()),
        None => (body, ""),
    };

    let mut flags: Vec<&str> = Vec::new();
    let mut positional: Vec<String> = Vec::new();
    for word in rest.split_whitespace() {
        if word.starts_with('-') && word.len() > 1 {
            flags.push(word);
        } else {
            positional.push(word.to_owned());
        }
    }
    let has = |f: &str| flags.contains(&f);
    let first = positional.first().cloned().unwrap_or_default();

    let directive = match head {
        "help" | "h" | "?" => Directive::Help,
        "exit" | "quit" | "q" => Directive::Exit,
        "blocks" | "ls" => Directive::Blocks {
            bookmarked_only: has("-b") || has("--bookmarked"),
        },
        "current" | "cwd" => Directive::Current,
        "session" => Directive::Session,
        "show" => match positional.first().and_then(|w| parse_ref(Some(w.as_str()))) {
            Some(r) => Directive::Show(r),
            None => Directive::Show(BlockRef::from(crate::block::BlockIndex(usize::MAX))),
        },
        "search" | "filter" => {
            let pattern = positional.join(" ");
            if pattern.is_empty() {
                return Err("search requires a query".to_owned());
            }
            Directive::Search(SearchDirective {
                pattern,
                kind: if has("-r") || has("--regex") {
                    QueryKind::Regex
                } else {
                    QueryKind::Substring
                },
                scope: if has("-c") || has("--command-only") {
                    SearchScope::Command
                } else if has("-o") || has("--output-only") {
                    SearchScope::Output
                } else {
                    SearchScope::All
                },
                case_sensitive: has("-s") || has("--case-sensitive"),
                successful_only: has("--ok"),
                bookmarked_only: has("-b") || has("--bookmarked"),
                interactive: head == "filter",
            })
        }
        "bookmark" | "bm" => {
            let set = if has("-on") {
                Some(true)
            } else if has("-off") {
                Some(false)
            } else {
                None
            };
            let target = match positional.first() {
                None => BookmarkTarget::Current,
                Some(w) => match parse_ref(Some(w.as_str())) {
                    Some(r) => BookmarkTarget::Ref(r),
                    None => return Err(format!("invalid block reference `{w}`")),
                },
            };
            Directive::Bookmark { target, set }
        }
        "share" | "export" => {
            let format = positional
                .iter()
                .find_map(|w| ShareFormat::parse(w))
                .unwrap_or_default();
            let all = has("-a") || has("--all");
            let target = positional
                .iter()
                .find(|w| ShareFormat::parse(w).is_none())
                .and_then(|w| parse_ref(Some(w.as_str())));
            Directive::Share { target, format, all }
        }
        "copy" => Directive::Copy {
            target: positional.first().and_then(|w| parse_ref(Some(w.as_str()))),
        },
        "complete" | "comp" => {
            let line = if positional.is_empty() {
                rest.to_owned()
            } else {
                rest.split_whitespace().collect::<Vec<_>>().join(" ")
            };
            Directive::Complete { line }
        }
        "highlight" | "hl" => {
            // A numeric positional is a block index; anything else names a language.
            let target = positional
                .iter()
                .find(|w| w.parse::<usize>().is_ok())
                .and_then(|w| parse_ref(Some(w.as_str())));
            let language = positional
                .iter()
                .find(|w| w.parse::<usize>().is_err())
                .cloned();
            Directive::Highlight {
                target,
                language,
                annotated: has("--annotated"),
            }
        }
        "nav" | "goto" | "j" | "k" => {
            let nav = match first.as_str() {
                "next" | "n" | "j" => NavDirective::Next,
                "prev" | "p" | "k" => NavDirective::Prev,
                "first" | "f" => NavDirective::First,
                "last" | "l" => NavDirective::Last,
                // Navigation is positional, so only numeric targets make sense here.
                other => match other.parse::<usize>() {
                    Ok(n) => NavDirective::Go(BlockRef::from(BlockIndex(n))),
                    Err(_) => return Err(format!("unknown navigation target `{other}`")),
                },
            };
            Directive::Nav(nav)
        }
        other => Directive::Unknown(other.to_owned()),
    };

    Ok(Some(directive))
}

fn parse_ref(word: Option<&str>) -> Option<BlockRef> {
    let w = word?;
    if w.is_empty() {
        return None;
    }
    Some(match w.parse::<usize>() {
        Ok(n) => BlockRef::from(BlockIndex(n)),
        Err(_) => BlockRef::from(crate::block::BlockId::from(w.to_owned())),
    })
}

#[cfg(test)]
#[path = "cli_tests.rs"]
mod tests;
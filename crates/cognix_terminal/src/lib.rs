//! # cognix_terminal
//!
//! Block-structured terminal output.
//!
//! The [`Block`] is the fundamental unit for organizing terminal output: every command and its
//! output are grouped into a single block that can be copied, searched, filtered, bookmarked,
//! shared, and navigated independently. This replaces the endless scroll of a traditional terminal
//! with structured, actionable output.
//!
//! ```
//! use cognix_terminal::{BlockOutput, ExitCode, Session};
//! use cognix_terminal::search::{search, SearchQuery};
//!
//! let session = Session::with_random_id();
//! let index = session.terminal.begin_block("git status");
//! session.terminal
//!     .with_block(index, |b| {
//!         b.set_output(BlockOutput::from_bytes(b"On branch main\n", b""));
//!         b.finish(ExitCode::from(0), 12);
//!     })
//!     .unwrap();
//!
//! let hits = search(&session.terminal, &SearchQuery::new("main")).unwrap();
//! assert_eq!(hits, vec![index]);
//! ```

pub mod block;
pub mod cli;
pub mod completer;
pub mod highlight;
pub mod runner;
pub mod search;
pub mod session;
pub mod share;

pub use block::{Block, BlockId, BlockIndex, BlockOutput, BlockState, OutputLine, Stream};
pub use completer::{complete, Completion, CompletionKind, Completions, SignatureRegistry};
pub use highlight::{highlight, highlight_path, HighlightSpan, TokenKind};
pub use runner::RunError;
pub use search::{search, search_detailed, SearchError, SearchQuery};
pub use session::{BlockRef, Navigator, Session, SessionError, TerminalSession};
pub use share::{share_block, ShareError, ShareFormat};

// Re-exported for convenience so callers do not need to depend on cognix_core directly.
pub use cognix_core::command::ExitCode;

/// Runs the command-line interface. Returns the process exit code.
pub async fn main(cli: cli::Cli) -> i32 {
    match cli.command {
        None => {
            let _ = cli::run_repl(None).await;
            0
        }
        Some(cli::Command::Repl { exec }) => {
            let _ = cli::run_repl(exec).await;
            0
        }
        Some(cli::Command::Run { command }) => {
            let line = command.join(" ");
            if runner::has_shell_metacharacters(&line) {
                eprintln!("error: shell metacharacters are not supported in a block");
                return 2;
            }
            match cli::repl::run_once(&line).await {
                Ok(code) => code,
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
        Some(cli::Command::Search { query, regex, case_sensitive }) => {
            let session = Session::with_random_id();
            let q = SearchQuery::new(query)
                .with_kind(if regex {
                    search::QueryKind::Regex
                } else {
                    search::QueryKind::Substring
                })
                .case_sensitive(case_sensitive);
            match search_detailed(&session.terminal, &q) {
                Ok(matches) => {
                    for m in matches {
                        println!("{}", m.text);
                    }
                    0
                }
                Err(e) => {
                    eprintln!("error: {e}");
                    1
                }
            }
        }
    }
}
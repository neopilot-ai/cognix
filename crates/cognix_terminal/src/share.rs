//! Exporting blocks so they can be shared outside the session.

use crate::block::Block;
use crate::session::TerminalSession;
use serde::{Deserialize, Serialize};

/// The output format for a share/export.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ShareFormat {
    /// A fenced markdown code block, suitable for pasting into a PR or chat.
    #[default]
    Markdown,
    /// Machine-readable JSON.
    Json,
    /// Raw output text, no decoration.
    PlainText,
}

impl ShareFormat {
    /// Parses a format name, accepting a few common aliases.
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Some(ShareFormat::Markdown),
            "json" => Some(ShareFormat::Json),
            "txt" | "plain" | "text" | "plaintext" => Some(ShareFormat::PlainText),
            _ => None,
        }
    }
}

/// Errors from sharing a block.
#[derive(Debug, thiserror::Error)]
pub enum ShareError {
    #[error("no block with id `{0}`")]
    UnknownBlockId(crate::block::BlockId),
    #[error("no block at index {0}")]
    UnknownBlockIndex(crate::block::BlockIndex),
    #[error("failed to serialize block: {0}")]
    Serialize(String),
}

/// A portable snapshot of a block, used by the JSON format.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct SharedBlock {
    id: String,
    index: usize,
    command: String,
    cwd: Option<String>,
    output: Vec<SharedLine>,
    bookmarked: bool,
    successful: bool,
    started_at: chrono::DateTime<chrono::Utc>,
    duration_ms: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct SharedLine {
    stream: String,
    text: String,
}

/// Renders a single block in `format`.
pub fn share_block(block: &Block, format: ShareFormat) -> Result<String, ShareError> {
    match format {
        ShareFormat::PlainText => Ok(block.output().to_plain_text()),
        ShareFormat::Markdown => Ok(to_markdown(block)),
        ShareFormat::Json => {
            let shared = SharedBlock {
                id: block.id().to_string(),
                index: block.index().0,
                command: block.command().to_owned(),
                cwd: block.cwd().map(str::to_owned),
                output: block
                    .output()
                    .lines()
                    .iter()
                    .map(|l| SharedLine {
                        stream: l.stream.to_string(),
                        text: l.text.clone(),
                    })
                    .collect(),
                bookmarked: block.is_bookmarked(),
                successful: block.is_successful(),
                started_at: block.started_at(),
                duration_ms: block.duration_ms(),
            };
            serde_json::to_string_pretty(&shared).map_err(|e| ShareError::Serialize(e.to_string()))
        }
    }
}

/// Renders one block as a markdown section.
///
/// The `command` line is emitted as an info-string fence rather than being merged into the body,
/// so a shared block round-trips back into a re-runnable block instead of an ambiguous blob of
/// text.
fn to_markdown(block: &Block) -> String {
    let mut out = String::new();
    out.push_str("## `");
    out.push_str(&block.command().replace('`', "'"));
    out.push_str("`\n\n");
    out.push_str("```console\n");
    for line in block.output() {
        out.push_str(&line.text);
        out.push('\n');
    }
    out.push_str("```\n");
    out
}

/// Renders several blocks as one document, separated by blank lines.
pub fn share_blocks(blocks: &[Block], format: ShareFormat) -> Result<String, ShareError> {
    let mut parts = Vec::with_capacity(blocks.len());
    for b in blocks {
        parts.push(share_block(b, format)?);
    }
    Ok(parts.join("\n"))
}

/// Shares every bookmarked block in `session`, or all of them if there are none.
pub fn share_bookmarked(session: &TerminalSession, format: ShareFormat) -> Result<String, ShareError> {
    let indices = session.bookmarked_indices();
    let blocks: Vec<Block> = indices
        .iter()
        .filter_map(|i| session.get_by_index(*i).ok())
        .collect();
    share_blocks(&blocks, format)
}

#[cfg(test)]
#[path = "share_tests.rs"]
mod tests;
//! The [`Block`]: Cognix's fundamental unit for organizing terminal output.
//!
//! Every command and its output are grouped into a single `Block` that can be copied, searched,
//! filtered, bookmarked, shared, and navigated independently — replacing the endless scroll of a
//! traditional terminal with structured, actionable output.

use std::fmt;
use std::str::FromStr;

use cognix_core::command::ExitCode;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A globally unique ID for a block.
///
/// Blocks created by running a command take the form `{session_id}-{n}`, where `n` is a
/// monotonically increasing counter for the session. Creating a UUID is comparatively expensive,
/// and block IDs are frequently minted from the shell's precmd hook where that cost is visible.
/// Manually created blocks get a UUID instead.
#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
#[serde(transparent)]
pub struct BlockId(String);

impl BlockId {
    /// Creates an ID for a block created by running a command in `session_id`.
    pub fn from_counter(session_id: &str, counter: u64) -> Self {
        Self(format!("{session_id}-{counter}"))
    }

    /// Creates an ID for a block created directly, rather than as the result of running a command.
    pub fn new() -> Self {
        Self(format!("manual-{}", Uuid::new_v4()))
    }

    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl Default for BlockId {
    fn default() -> Self {
        Self::new()
    }
}

impl FromStr for BlockId {
    type Err = std::convert::Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(s.to_owned()))
    }
}

impl From<String> for BlockId {
    fn from(value: String) -> Self {
        Self(value)
    }
}

impl From<BlockId> for String {
    fn from(value: BlockId) -> Self {
        value.0
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

/// A block's position within its session, independent of its [`BlockId`].
///
/// Indices are stable: a block keeps its index for the lifetime of the session even as later
/// blocks are appended, which is what lets navigation and selection survive scrolling.
#[derive(Default, Clone, Copy, Debug, Deserialize, PartialEq, PartialOrd, Eq, Ord, Hash, Serialize)]
pub struct BlockIndex(pub usize);

impl BlockIndex {
    pub fn zero() -> Self {
        Self(0)
    }

    pub fn next(self) -> Self {
        Self(self.0 + 1)
    }

    pub fn range_as_iter(range: std::ops::Range<BlockIndex>) -> impl Iterator<Item = BlockIndex> {
        (range.start.0..range.end.0).map(BlockIndex::from)
    }
}

impl From<usize> for BlockIndex {
    fn from(index: usize) -> Self {
        BlockIndex(index)
    }
}

impl From<BlockIndex> for usize {
    fn from(block_index: BlockIndex) -> Self {
        block_index.0
    }
}

impl fmt::Display for BlockIndex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Which stream a given line of output came from.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Stream {
    Stdout,
    Stderr,
}

impl fmt::Display for Stream {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stream::Stdout => "stdout",
            Stream::Stderr => "stderr",
        })
    }
}

/// A single line of a block's output, tagged with the stream it arrived on.
///
/// Keeping the stream per-line is what allows a block to preserve the interleaving of `stdout` and
/// `stderr`, which matters when diagnosing why a command failed.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct OutputLine {
    pub stream: Stream,
    pub text: String,
}

impl OutputLine {
    pub fn stdout(text: impl Into<String>) -> Self {
        Self {
            stream: Stream::Stdout,
            text: text.into(),
        }
    }

    pub fn stderr(text: impl Into<String>) -> Self {
        Self {
            stream: Stream::Stderr,
            text: text.into(),
        }
    }
}

impl fmt::Display for OutputLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.text)
    }
}

/// The captured output of a command, as an ordered list of stream-tagged lines.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct BlockOutput {
    lines: Vec<OutputLine>,
}

impl BlockOutput {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_lines(lines: impl IntoIterator<Item = OutputLine>) -> Self {
        Self {
            lines: lines.into_iter().collect(),
        }
    }

    /// Builds output from raw stdout/stderr byte buffers, splitting each into lines.
    ///
    /// A trailing newline does not produce a final empty line, and invalid UTF-8 is replaced
    /// rather than dropped, so binary-ish output still round-trips as text.
    pub fn from_bytes(stdout: &[u8], stderr: &[u8]) -> Self {
        let mut lines = Vec::new();
        for (stream, buf) in [(Stream::Stdout, stdout), (Stream::Stderr, stderr)] {
            for line in split_lines(buf) {
                lines.push(OutputLine {
                    stream,
                    text: line,
                });
            }
        }
        Self { lines }
    }

    pub fn push(&mut self, line: OutputLine) {
        self.lines.push(line);
    }

    pub fn lines(&self) -> &[OutputLine] {
        &self.lines
    }

    pub fn is_empty(&self) -> bool {
        self.lines.is_empty()
    }

    pub fn len(&self) -> usize {
        self.lines.len()
    }

    /// The output as plain text, with no stream markers.
    pub fn to_plain_text(&self) -> String {
        let mut out = String::new();
        for line in &self.lines {
            out.push_str(&line.text);
            out.push('\n');
        }
        out
    }
}

impl FromIterator<OutputLine> for BlockOutput {
    fn from_iter<I: IntoIterator<Item = OutputLine>>(iter: I) -> Self {
        Self::from_lines(iter)
    }
}

impl<'a> IntoIterator for &'a BlockOutput {
    type Item = &'a OutputLine;
    type IntoIter = std::slice::Iter<'a, OutputLine>;

    fn into_iter(self) -> Self::IntoIter {
        self.lines.iter()
    }
}

fn split_lines(buf: &[u8]) -> Vec<String> {
    if buf.is_empty() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(buf);
    let trimmed = text.strip_suffix('\n').unwrap_or(&text);
    if trimmed.is_empty() {
        return Vec::new();
    }
    trimmed
        .strip_suffix('\r')
        .unwrap_or(trimmed)
        .split('\n')
        .map(|l| l.strip_suffix('\r').unwrap_or(l).to_owned())
        .collect()
}

/// Whether a block has finished, and if not, why.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BlockState {
    /// Output is still being produced.
    Running,
    /// The command finished with the given exit code.
    Finished(ExitCode),
    /// The command was interrupted before it finished.
    Interrupted,
}

impl BlockState {
    pub fn is_running(&self) -> bool {
        matches!(self, BlockState::Running)
    }

    pub fn exit_code(&self) -> Option<ExitCode> {
        match self {
            BlockState::Finished(code) => Some(*code),
            _ => None,
        }
    }
}

/// A single command and the output it produced.
///
/// This is the unit everything else in the crate is built around: search, filtering, bookmarking,
/// sharing, and navigation all address blocks rather than a flat scrollback buffer.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct Block {
    id: BlockId,
    index: BlockIndex,
    command: String,
    cwd: Option<String>,
    output: BlockOutput,
    state: BlockState,
    bookmarked: bool,
    started_at: chrono::DateTime<chrono::Utc>,
    duration_ms: Option<u64>,
}

impl Block {
    /// Starts a new block for `command`.
    pub fn new(id: BlockId, index: BlockIndex, command: impl Into<String>) -> Self {
        Self {
            id,
            index,
            command: command.into(),
            cwd: None,
            output: BlockOutput::new(),
            state: BlockState::Running,
            bookmarked: false,
            started_at: chrono::Utc::now(),
            duration_ms: None,
        }
    }

    pub fn id(&self) -> &BlockId {
        &self.id
    }

    pub fn index(&self) -> BlockIndex {
        self.index
    }

    pub fn command(&self) -> &str {
        &self.command
    }

    pub fn set_cwd(&mut self, cwd: impl Into<String>) {
        self.cwd = Some(cwd.into());
    }

    pub fn cwd(&self) -> Option<&str> {
        self.cwd.as_deref()
    }

    pub fn output(&self) -> &BlockOutput {
        &self.output
    }

    pub fn state(&self) -> BlockState {
        self.state
    }

    pub fn is_bookmarked(&self) -> bool {
        self.bookmarked
    }

    pub fn started_at(&self) -> chrono::DateTime<chrono::Utc> {
        self.started_at
    }

    pub fn duration_ms(&self) -> Option<u64> {
        self.duration_ms
    }

    /// Toggles the bookmark and returns the new value.
    pub fn toggle_bookmark(&mut self) -> bool {
        self.bookmarked = !self.bookmarked;
        self.bookmarked
    }

    pub fn set_bookmarked(&mut self, bookmarked: bool) {
        self.bookmarked = bookmarked;
    }

    pub fn append(&mut self, line: OutputLine) {
        self.output.push(line);
    }

    pub fn set_output(&mut self, output: BlockOutput) {
        self.output = output;
    }

    /// Marks the block finished with `exit_code`, recording how long it ran.
    pub fn finish(&mut self, exit_code: ExitCode, duration_ms: u64) {
        self.state = BlockState::Finished(exit_code);
        self.duration_ms = Some(duration_ms);
    }

    pub fn interrupt(&mut self) {
        self.state = BlockState::Interrupted;
    }

    /// Whether the command completed successfully.
    pub fn is_successful(&self) -> bool {
        self.state.exit_code().is_some_and(|c| c.was_successful())
    }

    /// Whether the command produced anything on stderr.
    pub fn has_stderr(&self) -> bool {
        self.output.lines().iter().any(|l| l.stream == Stream::Stderr)
    }

    /// A one-line summary suitable for a block list.
    pub fn summary(&self) -> String {
        let marker = if self.bookmarked { "*" } else { " " };
        let status = match self.state {
            BlockState::Running => "running".to_owned(),
            BlockState::Finished(code) if code.was_successful() => "ok".to_owned(),
            BlockState::Finished(code) => format!("exit {code}"),
            BlockState::Interrupted => "interrupted".to_owned(),
        };
        format!(
            "{marker}{:>3} {} [{status}]",
            self.index, self.command
        )
    }
}

#[cfg(test)]
#[path = "block_tests.rs"]
mod tests;
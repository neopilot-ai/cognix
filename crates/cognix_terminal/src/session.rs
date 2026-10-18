//! The [`TerminalSession`]: an ordered collection of blocks plus the state needed to navigate
//! them.

use std::sync::atomic::{AtomicU64, Ordering};

use parking_lot::Mutex;

use crate::block::{Block, BlockId, BlockIndex};

/// Errors produced when addressing blocks that do not exist.
#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("no block with id `{0}`")]
    UnknownBlockId(BlockId),

    #[error("no block at index {0}")]
    UnknownBlockIndex(BlockIndex),

    #[error("the session has no blocks")]
    Empty,
}

/// A terminal session: the blocks produced so far, in the order they were created.
///
/// Blocks are append-only. Navigation state (which block the user is "at") lives alongside the
/// blocks so that walking backwards and forwards behaves like a real terminal history without
/// conflating "where you are" with "what exists".
pub struct TerminalSession {
    id: String,
    counter: AtomicU64,
    blocks: Mutex<Vec<Block>>,
}

impl TerminalSession {
    /// Creates a session whose block IDs are prefixed with `id`.
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            counter: AtomicU64::new(0),
            blocks: Mutex::new(Vec::new()),
        }
    }

    /// Creates a session with a random ID.
    pub fn with_random_id() -> Self {
        Self::new(uuid::Uuid::new_v4().to_string())
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    /// Starts a new block for `command` and appends it, returning its index.
    pub fn begin_block(&self, command: impl Into<String>) -> BlockIndex {
        let command = command.into();
        let counter = self.counter.fetch_add(1, Ordering::Relaxed);
        let id = BlockId::from_counter(&self.id, counter);
        let index = BlockIndex(self.blocks.lock().len());
        self.blocks.lock().push(Block::new(id, index, command));
        index
    }

    /// Appends `block`, assigning it the next index. Intended for restoring blocks from disk.
    pub fn push_block(&self, block: Block) {
        self.blocks.lock().push(block);
    }

    pub fn len(&self) -> usize {
        self.blocks.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.blocks.lock().is_empty()
    }

    /// A snapshot of every block, cloned so callers can inspect without holding the lock.
    pub fn blocks(&self) -> Vec<Block> {
        self.blocks.lock().clone()
    }

    /// The index of the most recently created block.
    pub fn last_index(&self) -> Result<BlockIndex, SessionError> {
        let blocks = self.blocks.lock();
        blocks
            .len()
            .checked_sub(1)
            .map(BlockIndex::from)
            .ok_or(SessionError::Empty)
    }

    pub fn get_by_index(&self, index: BlockIndex) -> Result<Block, SessionError> {
        self.blocks
            .lock()
            .get(index.0)
            .cloned()
            .ok_or(SessionError::UnknownBlockIndex(index))
    }

    pub fn get_by_id(&self, id: &BlockId) -> Result<Block, SessionError> {
        self.blocks
            .lock()
            .iter()
            .find(|b| b.id() == id)
            .cloned()
            .ok_or(SessionError::UnknownBlockId(id.clone()))
    }

    /// Applies `f` to the block at `index`, returning the result.
    pub fn with_block<T>(
        &self,
        index: BlockIndex,
        f: impl FnOnce(&mut Block) -> T,
    ) -> Result<T, SessionError> {
        let mut blocks = self.blocks.lock();
        let block = blocks
            .get_mut(index.0)
            .ok_or(SessionError::UnknownBlockIndex(index))?;
        Ok(f(block))
    }

    /// Toggles the bookmark on the block at `index`, returning the new value.
    pub fn toggle_bookmark(&self, index: BlockIndex) -> Result<bool, SessionError> {
        self.with_block(index, Block::toggle_bookmark)
    }

    /// The indices of every bookmarked block, in order.
    pub fn bookmarked_indices(&self) -> Vec<BlockIndex> {
        self.blocks
            .lock()
            .iter()
            .filter(|b| b.is_bookmarked())
            .map(Block::index)
            .collect()
    }

    /// The indices of every block still running.
    pub fn running_indices(&self) -> Vec<BlockIndex> {
        self.blocks
            .lock()
            .iter()
            .filter(|b| b.state().is_running())
            .map(Block::index)
            .collect()
    }

    /// Resolves a block reference, which may be either an index or an ID.
    pub fn resolve(&self, reference: &BlockRef) -> Result<Block, SessionError> {
        match reference {
            BlockRef::Index(i) => self.get_by_index(*i),
            BlockRef::Id(id) => self.get_by_id(id),
        }
    }
}

/// How a caller refers to a block: by position in the session, or by its globally unique ID.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BlockRef {
    Index(BlockIndex),
    Id(BlockId),
}

impl From<BlockIndex> for BlockRef {
    fn from(index: BlockIndex) -> Self {
        BlockRef::Index(index)
    }
}

impl From<BlockId> for BlockRef {
    fn from(id: BlockId) -> Self {
        BlockRef::Id(id)
    }
}

impl std::str::FromStr for BlockRef {
    type Err = std::convert::Infallible;

    /// Parses a bare integer as an index, and anything else as a block ID.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s.parse::<usize>() {
            Ok(n) => BlockRef::Index(BlockIndex(n)),
            Err(_) => BlockRef::Id(BlockId::from(s.to_owned())),
        })
    }
}

/// Where the user currently is within the session's blocks.
///
/// Kept separate from the block list so that `next`/`prev` behave like terminal history
/// navigation rather than mutating anything.
#[derive(Clone, Debug)]
pub struct Navigator {
    current: Option<BlockIndex>,
}

impl Navigator {
    pub fn new() -> Self {
        Self { current: None }
    }

    pub fn current(&self) -> Option<BlockIndex> {
        self.current
    }

    /// Jumps to an absolute index.
    pub fn seek(&mut self, index: BlockIndex) {
        self.current = Some(index);
    }

    /// Moves to `delta` blocks relative to the current position, clamped to the session bounds.
    pub fn move_by(&mut self, delta: isize, session_len: usize) {
        if session_len == 0 {
            self.current = None;
            return;
        }
        let base = match self.current {
            Some(c) => c.0 as isize,
            // With no position yet, moving forward lands on the first block and moving
            // backwards stays put, rather than starting from an implicit 0 and skipping it.
            None => {
                if delta > 0 {
                    self.current = Some(BlockIndex::zero());
                }
                return;
            }
        };
        let next = (base + delta).clamp(0, session_len as isize - 1);
        self.current = Some(BlockIndex::from(next as usize));
    }

    pub fn next(&mut self, session_len: usize) {
        self.move_by(1, session_len);
    }

    pub fn prev(&mut self, session_len: usize) {
        self.move_by(-1, session_len);
    }

    pub fn first(&mut self) {
        self.current = Some(BlockIndex::zero());
    }

    pub fn last(&mut self, session_len: usize) {
        self.current = session_len.checked_sub(1).map(BlockIndex::from);
    }
}

impl Default for Navigator {
    fn default() -> Self {
        Self::new()
    }
}

/// Convenience: the block at the navigator's position, if any.
impl TerminalSession {
    pub fn block_at(&self, nav: &Navigator) -> Result<Block, SessionError> {
        match nav.current() {
            Some(i) => self.get_by_index(i),
            None => Err(SessionError::Empty),
        }
    }
}

/// A handle combining a session with a navigator, which is what the CLI drives.
pub struct Session {
    pub terminal: TerminalSession,
    pub navigator: Mutex<Navigator>,
}

impl std::fmt::Debug for Session {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Session")
            .field("id", &self.terminal.id())
            .field("len", &self.terminal.len())
            .field("cursor", &self.navigator.lock().current())
            .finish()
    }
}

impl Session {
    pub fn new(id: impl Into<String>) -> Self {
        Self {
            terminal: TerminalSession::new(id),
            navigator: Mutex::new(Navigator::new()),
        }
    }

    pub fn with_random_id() -> Self {
        Self {
            terminal: TerminalSession::with_random_id(),
            navigator: Mutex::new(Navigator::new()),
        }
    }

    /// Runs `command`, capturing it into a new block, and navigates to that block.
    pub async fn run(&self, command: &str) -> Result<Block, crate::runner::RunError> {
        let index = self.terminal.begin_block(command);
        let block = crate::runner::run_in_block(
            &self.terminal,
            index,
            command,
            std::env::current_dir().ok(),
        )
        .await?;
        self.navigator.lock().seek(index);
        Ok(block)
    }

    /// Marks every block finished. Used when restoring or wrapping up a session.
    pub fn finalize_running(&self) {
        let indices = self.terminal.running_indices();
        for i in indices {
            let _ = self
                .terminal
                .with_block(i, |b| b.interrupt());
        }
    }
}

#[cfg(test)]
#[path = "session_tests.rs"]
mod tests;
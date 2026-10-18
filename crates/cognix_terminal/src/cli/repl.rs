//! The interactive REPL.

use std::io::{BufRead, IsTerminal, Write};

use crate::block::{Block, BlockIndex, Stream};
use crate::session::BlockRef;
use crate::cli::{
    parse_directive, BookmarkTarget, Directive, NavDirective, SearchDirective,
};
use crate::completer::SignatureRegistry;
use crate::search::{search, search_detailed, SearchQuery};
use crate::session::Session;
use crate::share::{share_block, share_blocks, ShareFormat};

/// Drives a session from an interactive prompt.
pub struct Repl {
    session: Session,
    registry: SignatureRegistry,
    out: Box<dyn Write>,
}

impl Repl {
    pub fn new(session: Session) -> Self {
        Self {
            session,
            registry: SignatureRegistry::builtin(),
            out: Box::new(std::io::stdout()),
        }
    }

    /// Replaces the command signature registry.
    pub fn with_registry(mut self, registry: SignatureRegistry) -> Self {
        self.registry = registry;
        self
    }

    /// Replaces the output sink. Used by tests to capture output.
    pub fn with_output(mut self, out: Box<dyn Write>) -> Self {
        self.out = out;
        self
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    fn line(&mut self, text: &str) {
        let _ = writeln!(self.out, "{text}");
    }

    /// The block index the cursor is on, if any.
    ///
    /// Returned by value rather than as a guard so callers can then take `&mut self` without
    /// holding a lock across the call.
    fn cursor(&self) -> Option<BlockIndex> {
        self.session.navigator.lock().current()
    }

    fn print_help(&mut self) {
        self.line("Cognix terminal — every command and its output are grouped into a Block.");
        self.line("");
        self.line("Anything without a leading ':' is run as a command and captured into a block.");
        self.line("");
        self.line("  :blocks [-b]           list blocks (-b bookmarks only)");
        self.line("  :show <ref>           show a block's output");
        self.line("  :current              show the block at the cursor");
        self.line("  :search <q> [-r] [-c|-o] [-s] [--ok] [-b]");
        self.line("  :filter <q>           like :search, but also moves the cursor");
        self.line("  :bookmark [ref] [-on|-off]");
        self.line("  :copy [ref]           copy a block to the clipboard");
        self.line("  :share [ref] [md|json|txt] [-a]");
        self.line("  :nav next|prev|first|last|<ref>");
        self.line("  :complete <line>      suggest completions for a line");
        self.line("  :highlight [ref] [lang] [--annotated]");
        self.line("  :session              print the session id");
        self.line("  :help                 this message");
        self.line("  :exit                 leave");
        self.line("");
        self.line("A block ref is either an index (0, 1, ...) or a block id.");
    }

    /// Executes a single input line. Returns `false` if the REPL should exit.
    pub async fn handle_line(&mut self, line: &str) -> bool {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return true;
        }

        let directive = match parse_directive(trimmed) {
            Ok(Some(d)) => d,
            Ok(None) => {
                self.run_command(trimmed).await;
                return true;
            }
            Err(e) => {
                self.line(&format!("error: {e}"));
                return true;
            }
        };

        match directive {
            Directive::Exit => return false,
            Directive::Help => self.print_help(),
            Directive::Session => {
                let id = self.session.terminal.id().to_owned();
                self.line(&id);
            }
            Directive::Blocks { bookmarked_only } => self.list_blocks(bookmarked_only),
            Directive::Current => {
                match self.cursor().and_then(|i| self.session.terminal.get_by_index(i).ok()) {
                    Some(b) => self.print_block(&b),
                    None => self.line("error: no current block"),
                }
            }
            Directive::Show(reference) => self.show(reference),
            Directive::Search(d) => self.do_search(d),
            Directive::Bookmark { target, set } => self.do_bookmark(target, set),
            Directive::Share { target, format, all } => self.do_share(target, format, all),
            Directive::Copy { target } => self.do_copy(target),
            Directive::Nav(nav) => self.do_nav(nav),
            Directive::Complete { line } => self.do_complete(&line),
            Directive::Highlight {
                target,
                language,
                annotated,
            } => self.do_highlight(target, language.as_deref(), annotated),
            Directive::Unknown(name) => {
                self.line(&format!("error: unknown directive `:{}` (try :help)", name))
            }
        }

        true
    }

    fn list_blocks(&mut self, bookmarked_only: bool) {
        let blocks = self.session.terminal.blocks();
        if blocks.is_empty() {
            self.line("no blocks yet");
            return;
        }
        for b in &blocks {
            if bookmarked_only && !b.is_bookmarked() {
                continue;
            }
            self.line(&b.summary());
        }
    }

    fn print_block(&mut self, block: &Block) {
        for line in block.output() {
            match line.stream {
                Stream::Stdout => self.line(&line.text),
                Stream::Stderr => self.line(&format!("! {}", line.text)),
            }
        }
        if let Some(code) = block.state().exit_code() {
            self.line(&format!("[exit {code}]"));
        }
    }

    fn show(&mut self, reference: BlockRef) {
        match self.session.terminal.resolve(&reference) {
            Ok(b) => self.print_block(&b),
            Err(e) => self.line(&format!("error: {e}")),
        }
    }

    fn do_search(&mut self, d: SearchDirective) {
        let query = SearchQuery::new(d.pattern.clone())
            .with_kind(d.kind)
            .with_scope(d.scope)
            .case_sensitive(d.case_sensitive)
            .successful_only(d.successful_only)
            .bookmarked_only(d.bookmarked_only);

        match search_detailed(&self.session.terminal, &query) {
            Ok(matches) => {
                self.line(&format!("{} match(es):", matches.len()));
                for m in matches.iter().take(50) {
                    match m.line_number {
                        Some(n) => self.line(&format!(
                            "  block {:>3} line {n:>3}  {}",
                            m.block_index, m.text
                        )),
                        None => self.line(&format!("  block {:>3} cmd        {}", m.block_index, m.text)),
                    }
                }
                if matches.len() > 50 {
                    self.line("  ... (truncated)");
                }

                if d.interactive() {
                    if let Some(first) = matches.first() {
                        self.session.navigator.lock().seek(first.block_index);
                        let msg = format!("cursor -> block {}", first.block_index);
                        self.line(&msg);
                    }
                }
            }
            Err(e) => self.line(&format!("error: {e}")),
        }
    }

    fn do_bookmark(&mut self, target: BookmarkTarget, set: Option<bool>) {
        let index = match self.resolve_target(target) {
            Ok(i) => i,
            Err(msg) => {
                self.line(&format!("error: {msg}"));
                return;
            }
        };

        let result = match set {
            Some(v) => self.session.terminal.with_block(index, |b| {
                b.set_bookmarked(v);
                v
            }),
            None => self.session.terminal.toggle_bookmark(index),
        };

        match result {
            Ok(true) => self.line(&format!("bookmarked block {index}")),
            Ok(false) => self.line(&format!("unbookmarked block {index}")),
            Err(e) => self.line(&format!("error: {e}")),
        }
    }

    fn do_copy(&mut self, target: Option<BlockRef>) {
        let index = match target.map_or_else(
            || Ok(None),
            |r| self.session.terminal.resolve(&r).map(Some),
        ) {
            Ok(Some(b)) => b.index(),
            Ok(None) => match self.cursor() {
                Some(i) => i,
                None => {
                    self.line("error: nothing to copy (no current block)");
                    return;
                }
            },
            Err(e) => {
                self.line(&format!("error: {e}"));
                return;
            }
        };

        let block = match self.session.terminal.get_by_index(index) {
            Ok(b) => b,
            Err(e) => {
                self.line(&format!("error: {e}"));
                return;
            }
        };

        let text = block.output().to_plain_text();
        match arboard::Clipboard::new().and_then(|mut c| c.set_text(text.clone())) {
            Ok(()) => {
                let msg = format!("copied block {index} to clipboard ({} bytes)", text.len());
                self.line(&msg);
            }
            Err(e) => {
                // Headless environments have no clipboard; printing is the useful fallback.
                self.line(&format!("clipboard unavailable ({e}); block {index} contents:"));
                self.print_block(&block);
            }
        }
    }

    fn do_share(&mut self, target: Option<BlockRef>, format: ShareFormat, all: bool) {
        let rendered = if all {
            let blocks = self.session.terminal.blocks();
            share_blocks(&blocks, format)
        } else {
            let index = match target {
                Some(r) => match self.session.terminal.resolve(&r) {
                    Ok(b) => b.index(),
                    Err(e) => {
                        self.line(&format!("error: {e}"));
                        return;
                    }
                },
                None => match self.cursor() {
                    Some(i) => i,
                    None => {
                        self.line("error: nothing to share (no current block)");
                        return;
                    }
                },
            };
            match self.session.terminal.get_by_index(index) {
                Ok(b) => share_block(&b, format),
                Err(e) => {
                    self.line(&format!("error: {e}"));
                    return;
                }
            }
        };

        match rendered {
            Ok(text) => {
                let _ = self.out.write_all(text.as_bytes());
                let _ = self.out.flush();
            }
            Err(e) => self.line(&format!("error: {e}")),
        }
    }

    /// Completes the last token of `line` against known programs, flags, and paths.
    fn do_complete(&mut self, line: &str) {
        let cwd = std::env::current_dir().unwrap_or_default();
        let mut known = self.known_commands();
        known.extend(extra_commands_on_path());

        let results = crate::completer::complete(line, &cwd, &known, &self.registry, 20);
        if results.is_empty() {
            self.line(&format!("no completions for `{line}`"));
            return;
        }

        let prefix = results.prefix();
        for c in results.items() {
            let detail = c.detail().map(|d| format!("  {d}")).unwrap_or_default();
            self.line(&format!("{:<24} {:<6}{detail}", c.text(), c.kind().label()));
        }

        if let Some(best) = results.best() {
            let completed = format!("{}{}", &line[..line.len() - prefix.len()], best.text());
            self.line(&format!("-> {completed}"));
        }
    }

    /// Programs available for completion: registry entries plus anything already run this session.
    fn known_commands(&self) -> std::collections::BTreeSet<String> {
        let mut known: std::collections::BTreeSet<String> =
            self.registry.names().map(str::to_owned).collect();
        for block in self.session.terminal.blocks() {
            if let Some(program) = block.command().split_whitespace().next() {
                known.insert(program.to_owned());
            }
        }
        known
    }

    fn do_highlight(
        &mut self,
        target: Option<BlockRef>,
        language: Option<&str>,
        annotated: bool,
    ) {
        let index = match self.resolve_ref_or_cursor(target) {
            Ok(i) => i,
            Err(msg) => {
                self.line(&format!("error: {msg}"));
                return;
            }
        };
        let block = match self.session.terminal.get_by_index(index) {
            Ok(b) => b,
            Err(e) => {
                self.line(&format!("error: {e}"));
                return;
            }
        };

        let source = block.output().to_plain_text();
        let lang = match language {
            Some(l) => Some(l.to_owned()),
            None => crate::highlight::detect_language(block.command())
                .map(str::to_owned),
        };
        let Some(lang) = lang else {
            self.line(&format!(
                "error: cannot detect a language for `{}`; pass one, e.g. `:highlight {index} rust`",
                block.command()
            ));
            return;
        };

        let mut highlighter = arborium::Highlighter::new();
        match crate::highlight::highlight(&mut highlighter, &lang, &source) {
            Ok(spans) => {
                let rendered = if annotated {
                    crate::highlight::to_annotated(&spans, &source)
                } else {
                    crate::highlight::to_ansi(&spans, &source)
                };
                let _ = self.out.write_all(rendered.as_bytes());
                let _ = self.out.flush();
            }
            Err(e) => self.line(&format!("error: {e}")),
        }
    }

    /// Resolves an optional ref, falling back to the cursor.
    fn resolve_ref_or_cursor(&self, target: Option<BlockRef>) -> Result<BlockIndex, String> {
        match target {
            Some(r) => self
                .session
                .terminal
                .resolve(&r)
                .map(|b| b.index())
                .map_err(|e| e.to_string()),
            None => self.cursor().ok_or_else(|| "no current block".to_owned()),
        }
    }

    fn do_nav(&mut self, nav: NavDirective) {
        let len = self.session.terminal.len();
        let mut nav_guard = self.session.navigator.lock();
        match nav {
            NavDirective::Next => nav_guard.next(len),
            NavDirective::Prev => nav_guard.prev(len),
            NavDirective::First => nav_guard.first(),
            NavDirective::Last => nav_guard.last(len),
            NavDirective::Go(reference) => match self.session.terminal.resolve(&reference) {
                Ok(b) => nav_guard.seek(b.index()),
                Err(e) => {
                    drop(nav_guard);
                    self.line(&format!("error: {e}"));
                    return;
                }
            },
        }
        let landed = nav_guard.current();
        drop(nav_guard);

        match landed {
            Some(i) => {
                let msg = format!("cursor -> block {i}");
                self.line(&msg);
            }
            None => self.line("cursor -> (empty)"),
        }
    }

    fn resolve_target(&self, target: BookmarkTarget) -> Result<crate::block::BlockIndex, String> {
        match target {
            BookmarkTarget::Current => self
                .session
                .navigator
                .lock()
                .current()
                .ok_or_else(|| "no current block".to_owned()),
            BookmarkTarget::Ref(r) => self
                .session
                .terminal
                .resolve(&r)
                .map(|b| b.index())
                .map_err(|e| e.to_string()),
        }
    }

    async fn run_command(&mut self, command: &str) {
        if crate::runner::has_shell_metacharacters(command) {
            self.line("error: shell metacharacters are not supported in a block");
            return;
        }

        let index = self.session.terminal.begin_block(command);
        let cwd = std::env::current_dir().ok();
        match crate::runner::run_in_block(&self.session.terminal, index, command, cwd).await {
            Ok(block) => {
                self.session.navigator.lock().seek(index);
                self.print_block(&block);
            }
            Err(e) => {
                self.line(&format!("error: {e}"));
            }
        }
    }

    /// The interactive read-eval-print loop. Returns when the user exits or stdin closes.
    pub async fn run(&mut self) -> std::io::Result<()> {
        let interactive = std::io::stdin().is_terminal();
        if interactive {
            self.print_help();
        }

        let stdin = std::io::stdin();
        let mut lines = stdin.lock().lines();
        loop {
            if interactive {
                let _ = write!(self.out, "cognix> ");
                let _ = self.out.flush();
            }

            let Some(line) = lines.next() else { break };
            let line = line?;
            if !self.handle_line(&line).await {
                break;
            }
        }

        self.session.finalize_running();
        let _ = self.out.flush();
        Ok(())
    }
}

/// Entry point for `cognix repl`.
pub async fn run_repl(initial: Option<String>) -> std::io::Result<()> {
    let mut repl = Repl::new(Session::with_random_id());
    if let Some(cmd) = initial {
        repl.handle_line(&cmd).await;
    }
    repl.run().await
}

/// Scans `$PATH` for executables, so completion covers programs the registry doesn't know about.
///
/// Deliberately best-effort: an unreadable or missing `$PATH` yields an empty set rather than an
/// error, because completion should never be the thing that breaks a shell.
pub fn extra_commands_on_path() -> std::collections::BTreeSet<String> {
    let mut found = std::collections::BTreeSet::new();
    let Some(path) = std::env::var_os("PATH") else {
        return found;
    };

    for dir in std::env::split_paths(&path) {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.is_empty() || name.starts_with('.') {
                continue;
            }
            if is_executable(&entry.path()) {
                found.insert(name);
            }
        }
    }
    found
}

/// Whether `path` is a regular file with any execute bit set.
fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

/// Convenience for a single non-interactive run.
pub async fn run_once(command: &str) -> Result<i32, crate::runner::RunError> {
    let captured = crate::runner::run_capture(command).await?;
    let mut stdout = std::io::stdout();
    for line in captured.lines() {
        let _ = writeln!(stdout, "{}", line.text);
    }
    Ok(captured.exit_code.value())
}

/// Reports whether any block in the session matched, used by tests.
pub fn has_match(session: &Session, pattern: &str) -> bool {
    search(&session.terminal, &SearchQuery::new(pattern)).is_ok()
}

#[cfg(test)]
#[path = "repl_tests.rs"]
mod tests;
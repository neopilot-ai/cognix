//! Running commands and capturing their output into blocks.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::Instant;

use cognix_core::command::ExitCode;
use tokio::process::Command;

use crate::block::{Block, BlockIndex, OutputLine, Stream};
use crate::session::TerminalSession;

/// Errors from running a command.
#[derive(Debug, thiserror::Error)]
pub enum RunError {
    #[error("command not found: {0}")]
    NotFound(String),

    #[error("failed to spawn command: {0}")]
    Spawn(String),

    #[error("no block at index {0}")]
    UnknownBlockIndex(BlockIndex),

    #[error("empty command")]
    EmptyCommand,
}

/// Runs `command` through the platform shell, appending its output to the block at `index`.
pub async fn run_in_block(
    session: &TerminalSession,
    index: BlockIndex,
    command: &str,
    cwd: Option<PathBuf>,
) -> Result<Block, RunError> {
    if command.trim().is_empty() {
        return Err(RunError::EmptyCommand);
    }

    if let Some(cwd) = &cwd {
        let _ = session.with_block(index, |b| b.set_cwd(cwd.display().to_string()));
    }

    let started = Instant::now();
    let (program, args) = shell_command(command);

    let mut cmd = Command::new(&program);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = cwd {
        cmd.current_dir(cwd);
    }

    let output = match cmd.output().await {
        Ok(o) => o,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(RunError::NotFound(program));
        }
        Err(e) => return Err(RunError::Spawn(e.to_string())),
    };

    let duration_ms = started.elapsed().as_millis() as u64;
    let exit_code = ExitCode::from(output.status.code().unwrap_or(-1));

    // stdout and stderr are drained separately, so ordering between the two streams is not
    // preserved; each stream keeps its internal order. Terminals that interleave these need
    // a merged pipe, which would change how the child sees its stdout.
    let mut lines = Vec::new();
    for line in crate::block::BlockOutput::from_bytes(&output.stdout, &output.stderr).lines() {
        lines.push(OutputLine {
            stream: line.stream,
            text: line.text.clone(),
        });
    }

    session
        .with_block(index, |b| {
            b.set_output(crate::block::BlockOutput::from_lines(lines));
            b.finish(exit_code, duration_ms);
        })
        .map_err(|_| RunError::UnknownBlockIndex(index))?;

    session.get_by_index(index).map_err(|_| RunError::UnknownBlockIndex(index))
}

/// Runs a command without a session, returning the captured output.
///
/// Useful for one-shot execution and for tests that only care about the output.
pub async fn run_capture(command: &str) -> Result<Captured, RunError> {
    if command.trim().is_empty() {
        return Err(RunError::EmptyCommand);
    }
    let started = Instant::now();
    let (program, args) = shell_command(command);
    let output = Command::new(&program)
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                RunError::NotFound(program.clone())
            } else {
                RunError::Spawn(e.to_string())
            }
        })?;

    let out = crate::block::BlockOutput::from_bytes(&output.stdout, &output.stderr);
    Ok(Captured {
        output: out,
        exit_code: ExitCode::from(output.status.code().unwrap_or(-1)),
        duration_ms: started.elapsed().as_millis() as u64,
    })
}

/// The result of a one-shot command execution.
#[derive(Debug)]
pub struct Captured {
    pub output: crate::block::BlockOutput,
    pub exit_code: ExitCode,
    pub duration_ms: u64,
}

impl Captured {
    pub fn lines(&self) -> &[OutputLine] {
        self.output.lines()
    }

    pub fn stdout(&self) -> String {
        self.output
            .lines()
            .iter()
            .filter(|l| l.stream == Stream::Stdout)
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }

    pub fn stderr(&self) -> String {
        self.output
            .lines()
            .iter()
            .filter(|l| l.stream == Stream::Stderr)
            .map(|l| l.text.as_str())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// Splits a command line into a program and arguments using POSIX shell-word rules.
///
/// Deliberately does *not* go through `sh -c`: shell metacharacters (`&&`, pipes, redirection)
/// are rejected by the CLI before reaching here, so a command is always a single program with
/// arguments. That keeps quoting predictable and avoids a shell injection surface.
fn shell_command(command: &str) -> (String, Vec<String>) {
    match shell_words::split(command) {
        Ok(parts) if !parts.is_empty() => (parts[0].clone(), parts[1..].to_vec()),
        // Quoted but empty (e.g. `""`), or unbalanced quotes: fall back to splitting on
        // whitespace so the user gets a real "command not found" rather than a silent no-op.
        _ => {
            let mut parts = command.split_whitespace().map(str::to_owned);
            match parts.next() {
                Some(program) => (program, parts.collect()),
                None => (String::new(), Vec::new()),
            }
        }
    }
}

/// Shell metacharacters that would change the meaning of a command if passed through.
///
/// The CLI rejects these rather than supporting them, because a Block is meant to capture one
/// program's output; a pipeline would produce output belonging to several commands.
const META: &[char] = &['|', '&', ';', '>', '<', '`', '$', '(', ')', '{', '}', '*', '?', '!', '\n'];

/// Returns whether `command` contains shell metacharacters.
pub fn has_shell_metacharacters(command: &str) -> bool {
    command.contains(META)
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;
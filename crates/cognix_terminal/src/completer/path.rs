//! Filesystem path completion.

use std::path::{Path, PathBuf};

/// A candidate filesystem entry.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PathCompletion {
    /// The text to insert, keeping whatever prefix the user already typed.
    pub text: String,
    /// Just the entry's file name.
    pub name: String,
    pub is_dir: bool,
}

/// Completes the path token `prefix` relative to `cwd`.
///
/// Returns an empty list rather than an error for unreadable directories: completion is a
/// best-effort affordance and should never interrupt typing.
pub fn complete_path(prefix: &str, cwd: &Path) -> Vec<PathCompletion> {
    let expanded = expand_tilde(prefix, cwd);
    let (dir_part, file_part) = split_prefix(&expanded);

    let search_dir: PathBuf = if dir_part.is_empty() {
        cwd.to_path_buf()
    } else {
        cwd.join(&dir_part)
    };

    let Ok(entries) = std::fs::read_dir(&search_dir) else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(&file_part) {
            continue;
        }
        // Skip dotfiles unless the user explicitly asked for one.
        if name.starts_with('.') && !file_part.starts_with('.') {
            continue;
        }
        let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

        let mut text = dir_part.clone();
        text.push_str(&name);
        if is_dir {
            text.push('/');
        }
        out.push(PathCompletion { text, name, is_dir });
    }

    // Directories first, then case-insensitive by name, so the common case is one keystroke away.
    out.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.name.cmp(&b.name))
    });
    out
}

/// Splits `prefix` into the directory portion and the file-name prefix being completed.
fn split_prefix(prefix: &str) -> (String, String) {
    match prefix.rfind('/') {
        Some(i) => (prefix[..=i].to_owned(), prefix[i + 1..].to_owned()),
        None => (String::new(), prefix.to_owned()),
    }
}

/// Expands a leading `~` using `cwd`'s home directory, leaving other prefixes untouched.
fn expand_tilde(prefix: &str, cwd: &Path) -> String {
    if prefix != "~" && !prefix.starts_with("~/") {
        return prefix.to_owned();
    }
    let Some(home) = dirs_home(cwd) else {
        return prefix.to_owned();
    };
    if prefix == "~" {
        return home.to_string_lossy().into_owned();
    }
    home.join(&prefix[2..]).to_string_lossy().into_owned()
}

/// Prefers `$HOME`, falling back to inferring it from `cwd` (e.g. `/home/me/...`).
fn dirs_home(cwd: &Path) -> Option<PathBuf> {
    if let Ok(home) = std::env::var("HOME") {
        if !home.is_empty() {
            return Some(PathBuf::from(home));
        }
    }
    // Walk up from cwd looking for a directory literally named `home` or `Users`.
    let mut current = Some(cwd);
    while let Some(dir) = current {
        if let Some(name) = dir.file_name().and_then(|n| n.to_str()) {
            if name == "home" || name == "Users" {
                return dir.parent().map(Path::to_path_buf);
            }
        }
        current = dir.parent();
    }
    None
}

#[cfg(test)]
#[path = "path_tests.rs"]
mod tests;
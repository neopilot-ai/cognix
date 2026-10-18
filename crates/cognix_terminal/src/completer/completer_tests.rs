use super::*;
use std::collections::BTreeSet;

fn commands(names: &[&str]) -> BTreeSet<String> {
    names.iter().map(|s| s.to_string()).collect()
}

#[test]
fn fuzzy_score_accepts_everything_for_an_empty_prefix() {
    assert_eq!(fuzzy_score("git", ""), Some(0));
}

#[test]
fn fuzzy_score_rejects_non_matches() {
    assert_eq!(fuzzy_score("git", "zzz"), None);
}

#[test]
fn fuzzy_score_prefers_prefix_over_scattered_match() {
    let prefix = fuzzy_score("git", "gi").unwrap();
    let scattered = fuzzy_score("great-ideal-trail", "git").unwrap();
    assert!(
        prefix > scattered,
        "prefix {prefix} should beat scattered {scattered}"
    );
}

#[test]
fn fuzzy_score_is_case_insensitive() {
    assert_eq!(fuzzy_score("Git", "git"), Some(fuzzy_score("git", "git").unwrap()));
}

#[test]
fn current_token_is_the_last_word() {
    assert_eq!(current_token("git status"), "status");
    assert_eq!(current_token("git"), "git");
    assert_eq!(current_token(""), "");
    assert_eq!(current_token("   "), "");
}

#[test]
fn token_start_is_the_offset_of_the_current_token() {
    assert_eq!(token_start("git status"), 4);
    assert_eq!(token_start("git"), 0);
    assert_eq!(token_start(""), 0);
}

#[test]
fn completes_command_names_fuzzily() {
    let known = commands(&["git", "cargo", "grep", "ls"]);
    let registry = SignatureRegistry::builtin();
    let results = complete("ca", std::path::Path::new("/"), &known, &registry, 10);

    let texts: Vec<&str> = results.items().iter().map(|c| c.text()).collect();
    assert!(texts.contains(&"cargo"), "got {texts:?}");
    assert!(results.best().unwrap().text() == "cargo");
}

#[test]
fn command_completions_carry_signature_detail() {
    let known = commands(&["git"]);
    let registry = SignatureRegistry::builtin();
    let results = complete("gi", std::path::Path::new("/"), &known, &registry, 10);
    let git = results.items().iter().find(|c| c.text() == "git").unwrap();
    assert_eq!(git.kind(), CompletionKind::Command);
    assert!(git.detail().unwrap().contains("version control"));
}

#[test]
fn empty_input_offers_nothing() {
    let known = commands(&["git"]);
    let registry = SignatureRegistry::new();
    let results = complete("", std::path::Path::new("/"), &known, &registry, 10);
    assert!(results.is_empty());
}

#[test]
fn flags_are_suggested_once_a_leading_dash_is_typed() {
    let known = commands(&["git"]);
    let registry = SignatureRegistry::builtin();
    let results = complete("git --st", std::path::Path::new("/"), &known, &registry, 10);

    let texts: Vec<&str> = results.items().iter().map(|c| c.text()).collect();
    assert_eq!(texts, vec!["--status"]);
    assert_eq!(results.best().unwrap().kind(), CompletionKind::Flag);
}

#[test]
fn flag_suggestions_are_scoped_to_the_command() {
    let known = commands(&["git", "cargo"]);
    let registry = SignatureRegistry::builtin();

    // --diff belongs to git only.
    let git = complete("git --di", std::path::Path::new("/"), &known, &registry, 10);
    assert!(git.items().iter().any(|c| c.text() == "--diff"));

    let cargo = complete("cargo --di", std::path::Path::new("/"), &known, &registry, 10);
    assert!(cargo.is_empty(), "git's --diff must not leak into cargo");
}

#[test]
fn flags_for_unknown_commands_are_empty() {
    let known = commands(&["nope"]);
    let registry = SignatureRegistry::builtin();
    let results = complete("nope --x", std::path::Path::new("/"), &known, &registry, 10);
    assert!(results.is_empty());
}

#[test]
fn later_tokens_complete_paths_not_commands() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha.txt"), "").unwrap();
    std::fs::write(dir.path().join("beta.txt"), "").unwrap();

    let known = commands(&["ls", "alpha"]);
    let registry = SignatureRegistry::new();
    let results = complete("ls al", dir.path(), &known, &registry, 10);

    let texts: Vec<&str> = results.items().iter().map(|c| c.text()).collect();
    assert_eq!(texts, vec!["alpha.txt"]);
    assert_eq!(results.best().unwrap().kind(), CompletionKind::Path);
}

#[test]
fn results_are_deduplicated_keeping_the_best_score() {
    let known = commands(&["git"]);
    let registry = SignatureRegistry::new();
    // "git" would be offered both as a command; duplicates must collapse to one entry.
    let results = complete("g", std::path::Path::new("/"), &known, &registry, 10);
    let count = results.items().iter().filter(|c| c.text() == "git").count();
    assert_eq!(count, 1);
}

#[test]
fn finalize_sorts_best_first_and_respects_the_limit() {
    let mut c = Completions::new("x");
    c.push(Completion::new(CompletionKind::Command, "low", 1));
    c.push(Completion::new(CompletionKind::Command, "high", 100));
    c.push(Completion::new(CompletionKind::Command, "mid", 50));

    let out = c.finalize(2);
    assert_eq!(out.len(), 2);
    assert_eq!(out.best().unwrap().text(), "high");
}

#[test]
fn natural_language_detection_sees_prose() {
    assert!(looks_like_natural_language("please show me all the open files"));
}

#[test]
fn natural_language_detection_ignores_short_input() {
    assert!(!looks_like_natural_language("git"));
    assert!(!looks_like_natural_language("ls -la"));
}

#[test]
fn natural_language_detection_ignores_shell_syntax() {
    assert!(!looks_like_natural_language("ls -la && git status"));
}
#[test]
fn an_absolute_path_first_token_completes_paths_not_commands() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("hostfile"), "").unwrap();
    let prefix = format!("{}/host", dir.path().display());

    let known = commands(&["host"]);
    let registry = SignatureRegistry::new();
    let results = complete(&prefix, std::path::Path::new("/"), &known, &registry, 10);

    let texts: Vec<&str> = results.items().iter().map(|c| c.text()).collect();
    assert_eq!(texts, vec![format!("{prefix}file")]);
    assert_eq!(results.best().unwrap().kind(), CompletionKind::Path);
}

#[test]
fn a_relative_path_first_token_completes_paths() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("notes.txt"), "").unwrap();

    let known = commands(&["notes"]);
    let registry = SignatureRegistry::new();
    let results = complete("./no", dir.path(), &known, &registry, 10);

    let texts: Vec<&str> = results.items().iter().map(|c| c.text()).collect();
    assert_eq!(texts, vec!["./notes.txt"]);
}

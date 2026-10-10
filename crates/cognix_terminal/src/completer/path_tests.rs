use super::*;

fn fixture() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("alpha.txt"), "").unwrap();
    std::fs::write(dir.path().join("beta.md"), "").unwrap();
    std::fs::create_dir(dir.path().join("subdir")).unwrap();
    std::fs::write(dir.path().join(".hidden"), "").unwrap();
    dir
}

#[test]
fn completes_a_partial_name() {
    let dir = fixture();
    let got = complete_path("al", dir.path());
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].text, "alpha.txt");
    assert_eq!(got[0].name, "alpha.txt");
    assert!(!got[0].is_dir);
}

#[test]
fn empty_prefix_lists_everything_visible() {
    let dir = fixture();
    let mut names: Vec<String> = complete_path("", dir.path()).into_iter().map(|c| c.name).collect();
    names.sort();
    assert_eq!(names, vec!["alpha.txt", "beta.md", "subdir"]);
}

#[test]
fn dotfiles_are_hidden_until_requested() {
    let dir = fixture();
    let names: Vec<String> = complete_path("", dir.path()).into_iter().map(|c| c.name).collect();
    assert!(!names.contains(&".hidden".to_string()));

    let explicit = complete_path(".h", dir.path());
    assert_eq!(explicit.len(), 1);
    assert_eq!(explicit[0].name, ".hidden");
}

#[test]
fn directories_sort_first_and_get_a_trailing_slash() {
    let dir = fixture();
    let got = complete_path("", dir.path());
    assert_eq!(got[0].name, "subdir", "directories should sort first");
    assert!(got[0].is_dir);
    assert!(got[0].text.ends_with('/'));
}

#[test]
fn nested_prefix_keeps_the_typed_directory() {
    let dir = fixture();
    let got = complete_path("subdir/", dir.path());
    assert!(got.is_empty(), "subdir is empty");

    std::fs::write(dir.path().join("subdir").join("inner.txt"), "").unwrap();
    let got = complete_path("subdir/in", dir.path());
    assert_eq!(got.len(), 1);
    assert_eq!(got[0].text, "subdir/inner.txt");
}

#[test]
fn a_trailing_slash_only_lists_directories() {
    let dir = fixture();
    let got = complete_path("", dir.path());
    let files: Vec<&PathCompletion> = got.iter().filter(|c| !c.is_dir).collect();
    assert_eq!(files.len(), 2);
}

#[test]
fn unreadable_directories_yield_nothing_rather_than_erroring() {
    let got = complete_path("/definitely/not/here/", std::path::Path::new("/"));
    assert!(got.is_empty());
}

#[test]
fn split_prefix_separates_directory_from_name() {
    assert_eq!(split_prefix("foo/bar"), ("foo/".to_owned(), "bar".to_owned()));
    assert_eq!(split_prefix("bar"), (String::new(), "bar".to_owned()));
    assert_eq!(split_prefix("/abs/"), ("/abs/".to_owned(), String::new()));
}

#[test]
fn tilde_expands_only_when_it_leads() {
    let home = tempfile::tempdir().unwrap();
    // SAFETY: single-threaded test setup for a process-scoped env var.
    unsafe { std::env::set_var("HOME", home.path()) };

    assert_eq!(expand_tilde("~/x", std::path::Path::new("/")), format!("{}/x", home.path().display()));
    assert_eq!(expand_tilde("~", std::path::Path::new("/")), home.path().to_string_lossy().into_owned());
    assert_eq!(expand_tilde("a~/b", std::path::Path::new("/")), "a~/b");
}

#[test]
fn matching_is_case_sensitive_on_disk() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("CamelCase"), "").unwrap();
    // Sorted case-insensitively, but the prefix match stays exact.
    assert_eq!(complete_path("Cam", dir.path()).len(), 1);
    assert_eq!(complete_path("cam", dir.path()).len(), 0);
}
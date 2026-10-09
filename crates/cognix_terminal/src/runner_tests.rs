use super::*;

#[test]
fn shell_words_splits_quoted_arguments() {
    let (program, args) = shell_command("echo 'hello world'");
    assert_eq!(program, "echo");
    assert_eq!(args, vec!["hello world"]);
}

#[test]
fn shell_command_with_no_arguments() {
    let (program, args) = shell_command("ls");
    assert_eq!(program, "ls");
    assert!(args.is_empty());
}

#[test]
fn shell_command_falls_back_on_unbalanced_quotes() {
    let (program, args) = shell_command("echo 'unclosed");
    assert_eq!(program, "echo");
    assert_eq!(args, vec!["'unclosed"]);
}

#[test]
fn metacharacter_detection_rejects_pipelines_and_redirection() {
    for cmd in [
        "ls | wc",
        "a && b",
        "echo hi > file",
        "cat < in",
        "echo `id`",
        "echo $HOME",
        "ls; rm",
        "echo *.rs",
    ] {
        assert!(has_shell_metacharacters(cmd), "should reject: {cmd}");
    }
}

#[test]
fn metacharacter_detection_allows_plain_commands() {
    for cmd in [
        "cargo build",
        "git commit -m 'fix bug'",
        "ls -la /tmp",
        "grep -rn foo .",
    ] {
        assert!(
            !has_shell_metacharacters(cmd),
            "should allow: {cmd}"
        );
    }
}

#[test]
fn run_capture_separates_stdout_and_stderr() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();

    let captured = rt
        .block_on(run_capture("sh -c 'echo out; echo err 1>&2'"))
        .unwrap();

    assert_eq!(captured.stdout(), "out");
    assert_eq!(captured.stderr(), "err");
    assert!(captured.exit_code.was_successful());
}

#[test]
fn run_capture_reports_nonzero_exit() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let captured = rt.block_on(run_capture("false")).unwrap();
    assert!(!captured.exit_code.was_successful());
}

#[test]
fn run_capture_rejects_empty_command() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    assert!(matches!(
        rt.block_on(run_capture("  ")),
        Err(RunError::EmptyCommand)
    ));
}

#[test]
fn run_in_block_records_cwd_when_supplied() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let session = TerminalSession::new("t");
    let index = session.begin_block("pwd");
    let dir = tempfile::tempdir().unwrap();
    rt.block_on(run_in_block(
        &session,
        index,
        "pwd",
        Some(dir.path().to_path_buf()),
    ))
    .unwrap();

    let block = session.get_by_index(index).unwrap();
    assert_eq!(block.cwd(), Some(dir.path().to_string_lossy().as_ref()));
    assert!(block.is_successful());
}

#[test]
fn run_in_block_unknown_index_errors() {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let session = TerminalSession::new("t");
    let err = rt
        .block_on(run_in_block(&session, BlockIndex(5), "echo hi", None))
        .unwrap_err();
    assert!(matches!(err, RunError::UnknownBlockIndex(BlockIndex(5))));
}
use std::process::Command;

fn daemon_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aegisai-runtime-daemon"))
}

#[test]
fn explicit_help_exits_successfully() {
    let output = daemon_command()
        .arg("--help")
        .output()
        .expect("daemon binary should run");

    assert!(
        output.status.success(),
        "expected --help to exit 0, got {:?}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("Usage: aegisai-runtime-daemon [options]"));
    assert!(stdout.contains("--repo-root <path>"));
    assert!(
        output.stderr.is_empty(),
        "help should not use error output:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn incomplete_argument_exits_nonzero() {
    let output = daemon_command()
        .arg("--repo-root")
        .output()
        .expect("daemon binary should run");

    assert!(
        !output.status.success(),
        "expected incomplete argument to exit nonzero"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--repo-root expects a path"),
        "stderr should report the validation error:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

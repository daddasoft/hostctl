use std::{
    fs,
    path::Path,
    process::{Command, Output},
};
use tempfile::tempdir;

fn run(path: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_hostctl"))
        .env(
            "HOSTCTL_STATE_DIR",
            path.parent().unwrap().join(".hostctl-state"),
        )
        .arg("--hosts")
        .arg(path)
        .args(arguments)
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn remove_preserves_alias_comment_crlf_and_creates_backup() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    let original = b"\xef\xbb\xbf# header\r\n127.0.0.1  localhost\tapp.local  # dev\r\n";
    fs::write(&hosts, original).unwrap();

    let output = run(&hosts, &["remove", "app.local"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fs::read(&hosts).unwrap(),
        b"\xef\xbb\xbf# header\r\n127.0.0.1  localhost  # dev\r\n"
    );
    assert!(stdout(&output).contains("Backup: hosts."));

    let backups = run(&hosts, &["backup", "list"]);
    assert!(backups.status.success(), "{}", stderr(&backups));
    assert!(stdout(&backups).contains("valid"));
}

#[test]
fn dry_run_prints_proposal_without_writing_or_backing_up() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    let original = b"127.0.0.1 localhost\n";
    fs::write(&hosts, original).unwrap();

    let output = run(&hosts, &["--dry-run", "add", "10.0.0.1", "app.local"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(fs::read(&hosts).unwrap(), original);
    assert!(stdout(&output).contains("+++ proposed"));
    assert!(stdout(&output).contains("10.0.0.1\tapp.local"));
    assert!(!directory.path().join(".hostctl-state").exists());
}

#[test]
fn overwrite_splits_target_from_other_aliases() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"10.0.0.1 app.local api.local\n").unwrap();

    let output = run(
        &hosts,
        &[
            "add",
            "10.0.0.2",
            "app.local",
            "--overwrite",
            "--comment",
            "new address",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(
        fs::read(&hosts).unwrap(),
        b"10.0.0.1 api.local\n10.0.0.2\tapp.local\t# new address\n"
    );
}

#[test]
fn restore_recovers_verified_automatic_backup() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    let original = b"127.0.0.1 localhost\n";
    fs::write(&hosts, original).unwrap();

    let changed = run(&hosts, &["add", "10.0.0.1", "app.local"]);
    assert!(changed.status.success(), "{}", stderr(&changed));
    let backup_id = stdout(&changed)
        .lines()
        .find_map(|line| line.strip_prefix("Backup: "))
        .unwrap()
        .to_string();

    let restored = run(&hosts, &["restore", &backup_id]);
    assert!(restored.status.success(), "{}", stderr(&restored));
    assert_eq!(fs::read(&hosts).unwrap(), original);
}

#[test]
fn invalid_hostname_and_comment_are_rejected_without_backup() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"").unwrap();

    let hostname = run(&hosts, &["add", "127.0.0.1", "bad_name.local"]);
    assert!(!hostname.status.success());
    assert!(stderr(&hostname).contains("hostname labels"));

    let comment = run(
        &hosts,
        &["add", "127.0.0.1", "app.local", "--comment", "bad\ncomment"],
    );
    assert!(!comment.status.success());
    assert!(stderr(&comment).contains("control characters"));
    assert!(!directory.path().join(".hostctl-state").exists());
}

#[test]
fn invalid_arguments_are_reported_before_filesystem_permissions() {
    let missing = Path::new("directory-that-does-not-exist").join("hosts");
    let output = run(&missing, &["add", "app.local", "127.0.0.1"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("'app.local' is not a valid IP address"));
    assert!(!stderr(&output).contains("lock file"));
}

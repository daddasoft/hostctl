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
    assert!(!stdout(&output).contains("Backup:"));
    assert!(!stderr(&output).contains("parent directory is protected"));
    assert!(stderr(&output).contains("info: executing command: remove"));

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
    assert!(!stdout(&changed).contains("Backup:"));

    let restored = run(&hosts, &["restore", "latest"]);
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

#[test]
fn entry_lifecycle_supports_queries_updates_and_toggles() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"127.0.0.1 localhost\n").unwrap();

    let added = run(
        &hosts,
        &[
            "add",
            "10.0.0.1",
            "app.local",
            "api.local",
            "--comment",
            "dev",
        ],
    );
    assert!(added.status.success(), "{}", stderr(&added));
    assert!(
        fs::read_to_string(&hosts)
            .unwrap()
            .contains("app.local api.local")
    );

    let get = run(&hosts, &["--format", "json", "get", "api.local"]);
    assert!(get.status.success(), "{}", stderr(&get));
    assert!(stdout(&get).contains("\"comment\": \"dev\""));

    let updated = run(
        &hosts,
        &[
            "update",
            "api.local",
            "--ip",
            "10.0.0.2",
            "--rename",
            "v2.local",
        ],
    );
    assert!(updated.status.success(), "{}", stderr(&updated));

    let disabled = run(&hosts, &["disable", "v2.local"]);
    assert!(disabled.status.success(), "{}", stderr(&disabled));
    let status = run(&hosts, &["status", "v2.local"]);
    assert!(stdout(&status).contains("disabled"));
    let enabled = run(&hosts, &["enable", "v2.local"]);
    assert!(enabled.status.success(), "{}", stderr(&enabled));

    let removed = run(&hosts, &["remove", "--ip", "10.0.0.2"]);
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert!(!fs::read_to_string(&hosts).unwrap().contains("v2.local"));
}

#[test]
fn ensure_present_is_idempotent() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"127.0.0.1 localhost\n").unwrap();

    for _ in 0..2 {
        let output = run(&hosts, &["ensure", "present", "10.0.0.1", "app.local"]);
        assert!(output.status.success(), "{}", stderr(&output));
    }
    assert_eq!(
        fs::read_to_string(&hosts)
            .unwrap()
            .matches("app.local")
            .count(),
        1
    );
}

#[test]
fn structured_export_and_replace_import_round_trip() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    let export = directory.path().join("entries.json");
    fs::write(&hosts, b"127.0.0.1 localhost\n10.0.0.1 app.local # dev\n").unwrap();

    let exported = run(
        &hosts,
        &[
            "export",
            "--file-format",
            "json",
            "--output",
            export.to_str().unwrap(),
        ],
    );
    assert!(exported.status.success(), "{}", stderr(&exported));
    fs::write(&hosts, b"192.0.2.1 old.local\n").unwrap();

    let imported = run(
        &hosts,
        &[
            "import",
            export.to_str().unwrap(),
            "--mode",
            "replace",
            "--yes",
        ],
    );
    assert!(imported.status.success(), "{}", stderr(&imported));
    let content = fs::read_to_string(&hosts).unwrap();
    assert!(content.contains("localhost"));
    assert!(content.contains("app.local"));
    assert!(!content.contains("old.local"));
}

#[test]
fn groups_and_profiles_manage_only_the_marked_block() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    let config = directory.path().join(".hostctl.toml");
    fs::write(&hosts, b"127.0.0.1 manual.local\n").unwrap();
    let config_arg = config.to_str().unwrap();

    let group = run(
        &hosts,
        &[
            "--config",
            config_arg,
            "group",
            "add",
            "dev",
            "10.0.0.1",
            "app.local",
        ],
    );
    assert!(group.status.success(), "{}", stderr(&group));
    let content = fs::read_to_string(&hosts).unwrap();
    assert!(content.contains("manual.local"));
    assert!(content.contains("# >>> hostctl managed >>>"));
    assert!(content.contains("app.local"));

    let profile = run(
        &hosts,
        &["--config", config_arg, "profile", "create", "work", "dev"],
    );
    assert!(profile.status.success(), "{}", stderr(&profile));
    let activate = run(
        &hosts,
        &["--config", config_arg, "profile", "activate", "work"],
    );
    assert!(activate.status.success(), "{}", stderr(&activate));

    let disabled = run(&hosts, &["--config", config_arg, "group", "disable", "dev"]);
    assert!(disabled.status.success(), "{}", stderr(&disabled));
    let content = fs::read_to_string(&hosts).unwrap();
    assert!(content.contains("manual.local"));
    assert!(!content.contains("10.0.0.1\tapp.local"));
}

#[test]
fn completions_and_man_page_are_generated() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    let man = directory.path().join("hostctl.1");
    fs::write(&hosts, b"").unwrap();

    let completion = run(&hosts, &["completion", "powershell"]);
    assert!(completion.status.success(), "{}", stderr(&completion));
    assert!(stdout(&completion).contains("hostctl"));
    let manual = run(&hosts, &["man", "--output", man.to_str().unwrap()]);
    assert!(manual.status.success(), "{}", stderr(&manual));
    assert!(fs::metadata(man).unwrap().len() > 100);
}

#[test]
fn stable_exit_codes_distinguish_invalid_and_missing() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"127.0.0.1 localhost\n").unwrap();

    let invalid = run(&hosts, &["add", "bad-ip", "app.local"]);
    assert_eq!(invalid.status.code(), Some(2));
    let missing = run(&hosts, &["get", "missing.local"]);
    assert_eq!(missing.status.code(), Some(3));
}

#[test]
fn duplicate_mapping_can_be_removed_from_one_ip() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"10.0.0.1 app.local\n10.0.0.2 app.local\n").unwrap();

    let removed = run(&hosts, &["remove", "app.local", "--from-ip", "10.0.0.1"]);
    assert!(removed.status.success(), "{}", stderr(&removed));
    assert_eq!(fs::read_to_string(&hosts).unwrap(), "10.0.0.2 app.local\n");
}

#[test]
fn dry_run_supports_machine_readable_output() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"127.0.0.1 localhost\n").unwrap();
    let output = run(
        &hosts,
        &[
            "--dry-run",
            "--format",
            "json",
            "add",
            "10.0.0.1",
            "app.local",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let value: serde_json::Value = serde_json::from_str(&stdout(&output)).unwrap();
    assert_eq!(value["changed"], true);
    assert!(value["proposed"].as_str().unwrap().contains("app.local"));
}

#[test]
fn quiet_suppresses_command_info_log() {
    let directory = tempdir().unwrap();
    let hosts = directory.path().join("hosts");
    fs::write(&hosts, b"127.0.0.1 localhost\n").unwrap();

    let output = run(&hosts, &["--quiet", "list"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(!stderr(&output).contains("info: executing command:"));
}

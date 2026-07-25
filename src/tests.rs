use super::*;
use std::{fs, io};
use tempfile::TempDir;

struct TestHosts {
    _directory: TempDir,
    path: PathBuf,
}

impl TestHosts {
    fn path(&self) -> &Path {
        &self.path
    }
}

fn setup_temp_hosts(content: &str) -> TestHosts {
    let directory = TempDir::new().unwrap();
    let path = directory.path().join("hosts");
    fs::write(&path, content).unwrap();
    TestHosts {
        _directory: directory,
        path,
    }
}

#[test]
fn add_valid_entry_and_create_backup() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n");
    let path = file.path();

    cmd_add(
        path,
        "192.168.1.10",
        "myserver.local",
        None,
        false,
        false,
        false,
    )
    .unwrap();

    let content = fs::read_to_string(path).unwrap();
    assert!(content.contains("192.168.1.10\tmyserver.local"));
    assert_eq!(
        fs::read_to_string(backup_path(path).unwrap()).unwrap(),
        "127.0.0.1\tlocalhost\n"
    );
}

#[test]
fn add_rejects_invalid_ip() {
    let file = setup_temp_hosts("");
    let result = cmd_add(
        file.path(),
        "not-an-ip",
        "myserver.local",
        None,
        false,
        false,
        false,
    );

    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidInput);
    assert!(!backup_path(file.path()).unwrap().exists());
}

#[test]
fn add_rejects_line_injection() {
    let file = setup_temp_hosts("");
    let hostname_result = cmd_add(
        file.path(),
        "127.0.0.1",
        "safe.local#bad",
        None,
        false,
        false,
        false,
    );
    let comment_result = cmd_add(
        file.path(),
        "127.0.0.1",
        "safe.local",
        Some("ok\n10.0.0.1 injected.local".to_string()),
        false,
        false,
        false,
    );

    assert_eq!(
        hostname_result.unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
    assert_eq!(
        comment_result.unwrap_err().kind(),
        io::ErrorKind::InvalidInput
    );
}

#[test]
fn duplicate_fails_without_writing_backup() {
    let file = setup_temp_hosts("192.168.1.10\tmyserver.local\n");
    let result = cmd_add(
        file.path(),
        "10.0.0.1",
        "myserver.local",
        None,
        false,
        false,
        false,
    );

    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
    assert!(!backup_path(file.path()).unwrap().exists());
}

#[test]
fn force_adds_duplicate() {
    let file = setup_temp_hosts("192.168.1.10\tmyserver.local\n");
    cmd_add(
        file.path(),
        "10.0.0.1",
        "myserver.local",
        None,
        true,
        false,
        false,
    )
    .unwrap();

    let content = fs::read_to_string(file.path()).unwrap();
    assert!(content.contains("192.168.1.10\tmyserver.local"));
    assert!(content.contains("10.0.0.1\tmyserver.local"));
}

#[test]
fn overwrite_preserves_unrelated_aliases_and_comments() {
    let file = setup_temp_hosts(
        "# header\r\n192.168.1.10\tkeep.local myserver.local\t# original\r\n10.0.0.2 myserver.local other.local\r\n",
    );
    cmd_add(
        file.path(),
        "10.0.0.1",
        "myserver.local",
        Some("new mapping".to_string()),
        false,
        true,
        false,
    )
    .unwrap();

    let content = fs::read_to_string(file.path()).unwrap();
    assert_eq!(
        content,
        "# header\r\n192.168.1.10\tkeep.local\t# original\r\n10.0.0.1\tmyserver.local\t# new mapping\r\n10.0.0.2\tother.local\r\n"
    );
}

#[test]
fn remove_preserves_other_aliases_comment_and_crlf() {
    let file = setup_temp_hosts(
        "127.0.0.1\tlocalhost\r\n192.168.1.10\tkeep.local myserver.local alias.local  # dev\r\n",
    );
    cmd_remove(file.path(), "myserver.local", false).unwrap();

    let content = fs::read_to_string(file.path()).unwrap();
    assert_eq!(
        content,
        "127.0.0.1\tlocalhost\r\n192.168.1.10\tkeep.local alias.local  # dev\r\n"
    );
}

#[test]
fn remove_deletes_a_line_when_it_has_no_other_aliases() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n192.168.1.10\tmyserver.local\n");
    cmd_remove(file.path(), "myserver.local", false).unwrap();

    assert_eq!(
        fs::read_to_string(file.path()).unwrap(),
        "127.0.0.1\tlocalhost\n"
    );
}

#[test]
fn remove_preserves_a_missing_final_newline() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n192.168.1.10\tmyserver.local");
    cmd_remove(file.path(), "myserver.local", false).unwrap();

    assert_eq!(
        fs::read_to_string(file.path()).unwrap(),
        "127.0.0.1\tlocalhost"
    );
}

#[test]
fn remove_reports_missing_hostname() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n");
    let result = cmd_remove(file.path(), "myserver.local", false);

    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
}

#[test]
fn dry_run_changes_neither_hosts_nor_backup() {
    let original = "127.0.0.1\tlocalhost\n";
    let file = setup_temp_hosts(original);
    cmd_add(
        file.path(),
        "127.0.0.1",
        "app.local",
        None,
        false,
        false,
        true,
    )
    .unwrap();

    assert_eq!(fs::read_to_string(file.path()).unwrap(), original);
    assert!(!backup_path(file.path()).unwrap().exists());
}

#[test]
fn undo_restores_previous_content_and_keeps_redo_backup() {
    let original = "127.0.0.1\tlocalhost\n";
    let file = setup_temp_hosts(original);
    cmd_add(
        file.path(),
        "127.0.0.1",
        "app.local",
        None,
        false,
        false,
        false,
    )
    .unwrap();
    let changed = fs::read_to_string(file.path()).unwrap();

    cmd_undo(file.path(), false).unwrap();

    assert_eq!(fs::read_to_string(file.path()).unwrap(), original);
    assert_eq!(
        fs::read_to_string(backup_path(file.path()).unwrap()).unwrap(),
        changed
    );
}

#[test]
fn undo_fails_cleanly_without_a_backup() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n");
    let result = cmd_undo(file.path(), false);

    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::NotFound);
    assert_eq!(
        fs::read_to_string(file.path()).unwrap(),
        "127.0.0.1\tlocalhost\n"
    );
}

#[test]
fn append_uses_existing_line_endings() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\r\n");
    cmd_add(
        file.path(),
        "127.0.0.1",
        "app.local",
        None,
        false,
        false,
        false,
    )
    .unwrap();

    assert_eq!(
        fs::read_to_string(file.path()).unwrap(),
        "127.0.0.1\tlocalhost\r\n127.0.0.1\tapp.local\r\n"
    );
}

#[test]
fn parser_preserves_unmodified_content_exactly() {
    let content = "# comment\r\n\r\n  127.0.0.1   localhost alias  # note\ninvalid line\n";
    assert_eq!(HostsFile::parse(content).render(), content);
}

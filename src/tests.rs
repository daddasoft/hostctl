use super::*;
use std::fs;
use std::io::{self, Write};
use tempfile::NamedTempFile;

fn setup_temp_hosts(content: &str) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    write!(file, "{}", content).unwrap();
    file
}

#[test]
fn test_cmd_add_valid() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n");
    let path = file.path().to_path_buf();

    let res = cmd_add(&path, "192.168.1.10", "myserver.local", None, false, false);
    assert!(res.is_ok());

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("192.168.1.10\tmyserver.local"));
}

#[test]
fn test_cmd_add_invalid_ip() {
    let file = setup_temp_hosts("");
    let path = file.path().to_path_buf();

    let res = cmd_add(&path, "not-an-ip", "myserver.local", None, false, false);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::InvalidInput);
}

#[test]
fn test_cmd_add_duplicate_fails() {
    let file = setup_temp_hosts("192.168.1.10\tmyserver.local\n");
    let path = file.path().to_path_buf();

    let res = cmd_add(&path, "10.0.0.1", "myserver.local", None, false, false);
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::AlreadyExists);
}

#[test]
fn test_cmd_add_duplicate_force() {
    let file = setup_temp_hosts("192.168.1.10\tmyserver.local\n");
    let path = file.path().to_path_buf();

    let res = cmd_add(&path, "10.0.0.1", "myserver.local", None, true, false);
    assert!(res.is_ok());

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("192.168.1.10\tmyserver.local"));
    assert!(content.contains("10.0.0.1\tmyserver.local"));
}

#[test]
fn test_cmd_add_overwrite() {
    let file = setup_temp_hosts("192.168.1.10\tmyserver.local\n");
    let path = file.path().to_path_buf();

    let res = cmd_add(&path, "10.0.0.1", "myserver.local", None, false, true);
    assert!(res.is_ok());

    let content = fs::read_to_string(&path).unwrap();
    assert!(!content.contains("192.168.1.10"));
    assert!(content.contains("10.0.0.1\tmyserver.local"));
}

#[test]
fn test_cmd_remove() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n192.168.1.10\tmyserver.local\n");
    let path = file.path().to_path_buf();

    let res = cmd_remove(&path, "myserver.local");
    assert!(res.is_ok());

    let content = fs::read_to_string(&path).unwrap();
    assert!(content.contains("localhost"));
    assert!(!content.contains("myserver.local"));
}

#[test]
fn test_cmd_remove_not_found() {
    let file = setup_temp_hosts("127.0.0.1\tlocalhost\n");
    let path = file.path().to_path_buf();

    let res = cmd_remove(&path, "myserver.local");
    assert!(res.is_err());
    assert_eq!(res.unwrap_err().kind(), io::ErrorKind::NotFound);
}

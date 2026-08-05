mod hosts;
mod storage;

use hosts::{HostsDocument, validate_add_input, validate_hostname};
use std::{
    io,
    path::{Path, PathBuf},
};

pub use hosts::Entry;
pub use storage::{BackupInfo, MutationResult, backup_directory};

pub fn add_entry(
    path: &Path,
    ip: &str,
    hostname: &str,
    comment: Option<&str>,
    force: bool,
    overwrite: bool,
    dry_run: bool,
) -> io::Result<MutationResult> {
    validate_add_input(ip, hostname, comment, force, overwrite)?;
    storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        document.add(ip, hostname, comment, force, overwrite)?;
        Ok(document.render())
    })
}

pub fn remove_entry(
    path: &Path,
    hostname: &str,
    dry_run: bool,
) -> io::Result<(MutationResult, usize)> {
    validate_hostname(hostname)?;
    let mut removed = 0;
    let result = storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        removed = document.remove(hostname)?;
        Ok(document.render())
    })?;
    Ok((result, removed))
}

pub fn list_entries(path: &Path) -> io::Result<Vec<Entry>> {
    let content = storage::read_utf8(path)?;
    Ok(HostsDocument::parse(&content).entries())
}

pub fn create_backup(path: &Path) -> io::Result<BackupInfo> {
    storage::create_backup(path)
}

pub fn list_backups(path: &Path) -> io::Result<Vec<BackupInfo>> {
    storage::list_backups(path)
}

pub fn restore_backup(
    path: &Path,
    backup_id: Option<&str>,
    dry_run: bool,
) -> io::Result<MutationResult> {
    storage::restore_backup(path, backup_id, dry_run)
}

pub fn default_hosts_path() -> PathBuf {
    #[cfg(windows)]
    {
        PathBuf::from(r"C:\Windows\System32\drivers\etc\hosts")
    }
    #[cfg(not(windows))]
    {
        PathBuf::from("/etc/hosts")
    }
}

pub mod config;
mod data;
mod hosts;
mod network;
mod storage;

use hosts::{HostsDocument, validate_add_input, validate_hostname};
use std::{
    io,
    path::{Path, PathBuf},
};

pub use data::{DataFormat, parse_entries, serialize_entries, validate_entries};
pub use hosts::Entry;
pub use network::{
    DoctorReport, Resolution, UpdateStatus, check_update, doctor, flush_dns, resolve,
    self_uninstall, self_update,
};
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

pub fn add_entries(
    path: &Path,
    ip: &str,
    hostnames: &[String],
    comment: Option<&str>,
    force: bool,
    overwrite: bool,
    dry_run: bool,
) -> io::Result<MutationResult> {
    let mut validator = HostsDocument::parse("");
    validator.add_many(ip, hostnames, comment, force, overwrite)?;
    storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        document.add_many(ip, hostnames, comment, force, overwrite)?;
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

pub fn list_all_entries(path: &Path) -> io::Result<Vec<Entry>> {
    let content = storage::read_utf8(path)?;
    Ok(HostsDocument::parse(&content).all_entries())
}

pub fn update_entry(
    path: &Path,
    hostname: &str,
    ip: Option<&str>,
    rename: Option<&str>,
    comment: Option<&str>,
    dry_run: bool,
) -> io::Result<MutationResult> {
    storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        document.update(hostname, ip, rename, comment)?;
        Ok(document.render())
    })
}

pub fn remove_by_ip(path: &Path, ip: &str, dry_run: bool) -> io::Result<(MutationResult, usize)> {
    let mut removed = 0;
    let result = storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        removed = document.remove_ip(ip)?;
        Ok(document.render())
    })?;
    Ok((result, removed))
}

pub fn remove_entry_from_ip(
    path: &Path,
    hostname: &str,
    ip: &str,
    dry_run: bool,
) -> io::Result<(MutationResult, usize)> {
    let mut removed = 0;
    let result = storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        removed = document.remove_from_ip(hostname, ip)?;
        Ok(document.render())
    })?;
    Ok((result, removed))
}

pub fn set_entry_enabled(
    path: &Path,
    hostname: &str,
    enabled: bool,
    dry_run: bool,
) -> io::Result<MutationResult> {
    storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        document.set_enabled(hostname, enabled)?;
        Ok(document.render())
    })
}

pub fn import_entries(
    path: &Path,
    entries: &[Entry],
    replace: bool,
    dry_run: bool,
) -> io::Result<MutationResult> {
    validate_entries(entries)?;
    storage::mutate(path, dry_run, |content| {
        let mut document = HostsDocument::parse(content);
        if replace {
            document.replace_active(entries)?;
        } else {
            for entry in entries {
                document.add_many(
                    &entry.ip,
                    &entry.hostnames,
                    entry.comment.as_deref(),
                    false,
                    true,
                )?;
                if entry.disabled {
                    for hostname in &entry.hostnames {
                        document.set_enabled(hostname, false)?;
                    }
                }
            }
        }
        Ok(document.render())
    })
}

pub fn inspect_hosts(path: &Path) -> io::Result<(Vec<Entry>, usize)> {
    let content = storage::read_utf8(path)?;
    let document = HostsDocument::parse(&content);
    Ok((document.all_entries(), document.malformed_line_count()))
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

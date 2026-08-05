use crate::{Entry, backup_directory, inspect_hosts};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::OpenOptions,
    io,
    net::{IpAddr, ToSocketAddrs},
    path::{Path, PathBuf},
    process::Command,
};

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub hosts_path: PathBuf,
    pub readable: bool,
    pub writable: bool,
    pub entry_count: usize,
    pub disabled_count: usize,
    pub malformed_lines: usize,
    pub duplicate_hostnames: Vec<String>,
    pub backup_directory: Option<PathBuf>,
    pub healthy: bool,
}

#[derive(Debug, Serialize)]
pub struct Resolution {
    pub hostname: String,
    pub hosts_file: Vec<IpAddr>,
    pub system: Vec<IpAddr>,
    pub conflict: bool,
}

#[derive(Debug, Serialize)]
pub struct UpdateStatus {
    pub current: String,
    pub latest: String,
    pub update_available: bool,
}

pub fn doctor(path: &Path) -> io::Result<DoctorReport> {
    let readable = std::fs::File::open(path).is_ok();
    let writable = OpenOptions::new().write(true).open(path).is_ok();
    let (entries, malformed_lines) = inspect_hosts(path)?;
    let mut counts = BTreeMap::<String, usize>::new();
    for entry in &entries {
        for hostname in &entry.hostnames {
            *counts.entry(hostname.to_ascii_lowercase()).or_default() += 1;
        }
    }
    let duplicate_hostnames = counts
        .into_iter()
        .filter_map(|(hostname, count)| (count > 1).then_some(hostname))
        .collect::<Vec<_>>();
    let disabled_count = entries.iter().filter(|entry| entry.disabled).count();
    let healthy = readable && malformed_lines == 0 && duplicate_hostnames.is_empty();
    Ok(DoctorReport {
        hosts_path: path.to_path_buf(),
        readable,
        writable,
        entry_count: entries.len(),
        disabled_count,
        malformed_lines,
        duplicate_hostnames,
        backup_directory: backup_directory(path).ok(),
        healthy,
    })
}

pub fn resolve(path: &Path, hostname: &str) -> io::Result<Resolution> {
    crate::hosts::validate_hostname(hostname)?;
    let entries = crate::list_entries(path)?;
    let hosts_file = matching_ips(&entries, hostname);
    let system = (hostname, 0)
        .to_socket_addrs()
        .map(|addresses| {
            addresses
                .map(|address| address.ip())
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default()
        .into_iter()
        .collect::<Vec<_>>();
    let conflict = !hosts_file.is_empty()
        && !system.is_empty()
        && hosts_file.iter().collect::<BTreeSet<_>>() != system.iter().collect::<BTreeSet<_>>();
    Ok(Resolution {
        hostname: hostname.to_string(),
        hosts_file,
        system,
        conflict,
    })
}

pub fn flush_dns() -> io::Result<()> {
    #[cfg(windows)]
    return run_status("ipconfig", &["/flushdns"]);

    #[cfg(target_os = "macos")]
    {
        run_status("dscacheutil", &["-flushcache"])?;
        return run_status("killall", &["-HUP", "mDNSResponder"]);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if command_exists("resolvectl") {
            return run_status("resolvectl", &["flush-caches"]);
        }
        if command_exists("systemd-resolve") {
            return run_status("systemd-resolve", &["--flush-caches"]);
        }
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no supported DNS cache tool found (tried resolvectl and systemd-resolve)",
        ))
    }
}

pub fn self_update() -> io::Result<()> {
    #[cfg(windows)]
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "irm https://raw.githubusercontent.com/daddasoft/hostctl/main/install.ps1 | iex",
        ])
        .status();

    #[cfg(not(windows))]
    let status = Command::new("sh")
        .args([
            "-c",
            "curl -fsSL https://raw.githubusercontent.com/daddasoft/hostctl/main/install.sh | sh",
        ])
        .status();

    let status =
        status.map_err(|error| io::Error::other(format!("cannot start updater: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "installer exited with status {status}"
        )))
    }
}

pub fn check_update() -> io::Result<UpdateStatus> {
    #[cfg(windows)]
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            "(Invoke-RestMethod https://api.github.com/repos/daddasoft/hostctl/releases/latest).tag_name",
        ])
        .output();

    #[cfg(not(windows))]
    let output = Command::new("curl")
        .args([
            "-fsSL",
            "https://api.github.com/repos/daddasoft/hostctl/releases/latest",
        ])
        .output();

    let output =
        output.map_err(|error| io::Error::other(format!("cannot check updates: {error}")))?;
    if !output.status.success() {
        return Err(io::Error::other(format!(
            "update check exited with status {}",
            output.status
        )));
    }

    #[cfg(windows)]
    let latest = String::from_utf8_lossy(&output.stdout).trim().to_string();

    #[cfg(not(windows))]
    let latest = serde_json::from_slice::<serde_json::Value>(&output.stdout)
        .ok()
        .and_then(|value| value["tag_name"].as_str().map(str::to_owned))
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "release has no tag_name"))?;

    let current = env!("CARGO_PKG_VERSION").to_string();
    let normalized_latest = latest.strip_prefix('v').unwrap_or(&latest);
    Ok(UpdateStatus {
        update_available: normalized_latest != current,
        current,
        latest,
    })
}

pub fn self_uninstall() -> io::Result<()> {
    #[cfg(windows)]
    let child = Command::new("powershell")
        .args([
            "-NoProfile",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            "Start-Sleep -Milliseconds 750; irm https://raw.githubusercontent.com/daddasoft/hostctl/main/uninstall.ps1 | iex",
        ])
        .spawn();

    #[cfg(not(windows))]
    let child = Command::new("sh")
        .args([
            "-c",
            "curl -fsSL https://raw.githubusercontent.com/daddasoft/hostctl/main/uninstall.sh | sh",
        ])
        .spawn();

    child
        .map(|_| ())
        .map_err(|error| io::Error::other(format!("cannot start uninstaller: {error}")))
}

fn matching_ips(entries: &[Entry], hostname: &str) -> Vec<IpAddr> {
    entries
        .iter()
        .filter(|entry| {
            entry
                .hostnames
                .iter()
                .any(|value| value.eq_ignore_ascii_case(hostname))
        })
        .filter_map(|entry| entry.ip.parse().ok())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn run_status(program: &str, arguments: &[&str]) -> io::Result<()> {
    let status = Command::new(program)
        .args(arguments)
        .status()
        .map_err(|error| io::Error::other(format!("cannot run {program}: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "{program} exited with status {status}"
        )))
    }
}

#[cfg(all(unix, not(target_os = "macos")))]
fn command_exists(program: &str) -> bool {
    Command::new(program).arg("--help").output().is_ok()
}

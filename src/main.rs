use clap::{Parser, Subcommand};
use std::{
    ffi::OsString,
    fs,
    io::{self, Write},
    net::IpAddr,
    path::{Path, PathBuf},
};
use tempfile::NamedTempFile;

#[cfg(windows)]
const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";

#[cfg(not(windows))]
const HOSTS_PATH: &str = "/etc/hosts";

#[cfg(windows)]
const DEFAULT_NEWLINE: &str = "\r\n";

#[cfg(not(windows))]
const DEFAULT_NEWLINE: &str = "\n";

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "hostctl",
    version,
    about = "Manage the system hosts file safely",
    long_about = None,
)]
struct Cli {
    /// Path to the hosts file (defaults to the system hosts file)
    #[arg(long, global = true)]
    hosts: Option<PathBuf>,

    /// Preview a change without writing the hosts file or its backup
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a new entry (e.g. hostctl add 127.0.0.1 toto.local)
    Add {
        /// IP address (IPv4 or IPv6)
        ip: String,
        /// Hostname to map to the IP
        hostname: String,
        /// Optional inline comment
        #[arg(short, long)]
        comment: Option<String>,
        /// Allow adding a second entry even when the hostname already exists
        #[arg(long)]
        force: bool,
        /// Replace every existing mapping for the hostname with this one
        #[arg(short, long)]
        overwrite: bool,
    },

    /// Remove a hostname while preserving other aliases on the same line
    Remove {
        /// Hostname to remove
        hostname: String,
    },

    /// List all non-comment entries
    List,

    /// Restore the hosts file from the most recent automatic backup
    Undo,
}

// ── Hosts-file model ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, PartialEq, Eq)]
struct HostsLine {
    text: String,
    ending: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct HostsFile {
    lines: Vec<HostsLine>,
}

#[derive(Debug, PartialEq, Eq)]
struct ParsedEntry {
    leading: String,
    ip: String,
    hostnames: Vec<String>,
    suffix: String,
}

impl ParsedEntry {
    fn parse(line: &str) -> Option<Self> {
        let trimmed = line.trim_start();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            return None;
        }

        let comment_start = line.find('#').unwrap_or(line.len());
        let data = &line[..comment_start];
        let data_without_trailing_space = data.trim_end();
        let leading_len =
            data_without_trailing_space.len() - data_without_trailing_space.trim_start().len();
        let leading = &data_without_trailing_space[..leading_len];
        let fields: Vec<&str> = data_without_trailing_space[leading_len..]
            .split_whitespace()
            .collect();

        if fields.len() < 2 || fields[0].parse::<IpAddr>().is_err() {
            return None;
        }

        Some(Self {
            leading: leading.to_string(),
            ip: fields[0].to_string(),
            hostnames: fields[1..]
                .iter()
                .map(|field| (*field).to_string())
                .collect(),
            suffix: line[data_without_trailing_space.len()..].to_string(),
        })
    }

    fn render(&self) -> String {
        format!(
            "{}{}\t{}{}",
            self.leading,
            self.ip,
            self.hostnames.join(" "),
            self.suffix
        )
    }

    fn matching_hostname_count(&self, hostname: &str) -> usize {
        self.hostnames
            .iter()
            .filter(|candidate| candidate.eq_ignore_ascii_case(hostname))
            .count()
    }
}

impl HostsFile {
    fn parse(content: &str) -> Self {
        let lines = content
            .split_inclusive('\n')
            .map(|segment| {
                if let Some(without_lf) = segment.strip_suffix('\n') {
                    if let Some(text) = without_lf.strip_suffix('\r') {
                        HostsLine {
                            text: text.to_string(),
                            ending: "\r\n".to_string(),
                        }
                    } else {
                        HostsLine {
                            text: without_lf.to_string(),
                            ending: "\n".to_string(),
                        }
                    }
                } else {
                    HostsLine {
                        text: segment.to_string(),
                        ending: String::new(),
                    }
                }
            })
            .collect();

        Self { lines }
    }

    fn render(&self) -> String {
        self.lines
            .iter()
            .map(|line| format!("{}{}", line.text, line.ending))
            .collect()
    }

    fn preferred_newline(&self) -> String {
        self.lines
            .iter()
            .find(|line| !line.ending.is_empty())
            .map(|line| line.ending.clone())
            .unwrap_or_else(|| DEFAULT_NEWLINE.to_string())
    }

    fn hostname_count(&self, hostname: &str) -> usize {
        self.lines
            .iter()
            .filter_map(|line| ParsedEntry::parse(&line.text))
            .map(|entry| entry.matching_hostname_count(hostname))
            .sum()
    }

    fn append_entry(&mut self, line: String) {
        let newline = self.preferred_newline();
        if let Some(last) = self.lines.last_mut()
            && last.ending.is_empty()
        {
            last.ending = newline.clone();
        }
        self.lines.push(HostsLine {
            text: line,
            ending: newline,
        });
    }

    fn remove_hostname(&mut self, hostname: &str) -> usize {
        let had_final_newline = self
            .lines
            .last()
            .is_some_and(|line| !line.ending.is_empty());
        let newline = self.preferred_newline();
        let mut removed = 0;
        let mut output = Vec::with_capacity(self.lines.len());

        for mut line in self.lines.drain(..) {
            let Some(mut entry) = ParsedEntry::parse(&line.text) else {
                output.push(line);
                continue;
            };

            let matches = entry.matching_hostname_count(hostname);
            if matches == 0 {
                output.push(line);
                continue;
            }

            removed += matches;
            entry
                .hostnames
                .retain(|candidate| !candidate.eq_ignore_ascii_case(hostname));
            if !entry.hostnames.is_empty() {
                line.text = entry.render();
                output.push(line);
            }
        }

        preserve_final_newline(&mut output, had_final_newline, &newline);
        self.lines = output;
        removed
    }

    fn overwrite_hostname(&mut self, hostname: &str, new_line: &str) -> usize {
        let had_final_newline = self
            .lines
            .last()
            .is_some_and(|line| !line.ending.is_empty());
        let newline = self.preferred_newline();
        let mut replaced = 0;
        let mut inserted = false;
        let mut output = Vec::with_capacity(self.lines.len() + 1);

        for mut line in self.lines.drain(..) {
            let Some(mut entry) = ParsedEntry::parse(&line.text) else {
                output.push(line);
                continue;
            };

            let matches = entry.matching_hostname_count(hostname);
            if matches == 0 {
                output.push(line);
                continue;
            }

            replaced += matches;
            entry
                .hostnames
                .retain(|candidate| !candidate.eq_ignore_ascii_case(hostname));

            if inserted {
                if !entry.hostnames.is_empty() {
                    line.text = entry.render();
                    output.push(line);
                }
                continue;
            }

            inserted = true;
            if entry.hostnames.is_empty() {
                line.text = new_line.to_string();
                output.push(line);
            } else {
                let original_ending = line.ending.clone();
                line.text = entry.render();
                if line.ending.is_empty() {
                    line.ending = newline.clone();
                }
                output.push(line);
                output.push(HostsLine {
                    text: new_line.to_string(),
                    ending: original_ending,
                });
            }
        }

        preserve_final_newline(&mut output, had_final_newline, &newline);
        self.lines = output;
        replaced
    }
}

fn preserve_final_newline(lines: &mut [HostsLine], had_final_newline: bool, newline: &str) {
    if let Some(last) = lines.last_mut() {
        if had_final_newline && last.ending.is_empty() {
            last.ending = newline.to_string();
        } else if !had_final_newline {
            last.ending.clear();
        }
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    let hosts_path = cli.hosts.unwrap_or_else(|| PathBuf::from(HOSTS_PATH));

    let result = match cli.command {
        Command::Add {
            ip,
            hostname,
            comment,
            force,
            overwrite,
        } => cmd_add(
            &hosts_path,
            &ip,
            &hostname,
            comment,
            force,
            overwrite,
            cli.dry_run,
        ),
        Command::Remove { hostname } => cmd_remove(&hosts_path, &hostname, cli.dry_run),
        Command::List => cmd_list(&hosts_path),
        Command::Undo => cmd_undo(&hosts_path, cli.dry_run),
    };

    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn cmd_add(
    path: &Path,
    ip: &str,
    hostname: &str,
    comment: Option<String>,
    force: bool,
    overwrite: bool,
    dry_run: bool,
) -> io::Result<()> {
    if force && overwrite {
        return invalid_input("--force and --overwrite cannot be used together");
    }
    if ip.parse::<IpAddr>().is_err() {
        return invalid_input(format!("'{ip}' is not a valid IP address"));
    }
    validate_hostname(hostname)?;
    if let Some(comment) = &comment
        && (comment.contains('\n') || comment.contains('\r'))
    {
        return invalid_input("comment must not contain a line break");
    }

    let current = read_hosts(path)?;
    let mut hosts = HostsFile::parse(&current);
    let new_line = match &comment {
        Some(comment) => format!("{ip}\t{hostname}\t# {comment}"),
        None => format!("{ip}\t{hostname}"),
    };
    let duplicate_count = hosts.hostname_count(hostname);

    let action = if duplicate_count > 0 && overwrite {
        hosts.overwrite_hostname(hostname, &new_line);
        format!("overwrite {duplicate_count} mapping(s) for '{hostname}'")
    } else if duplicate_count > 0 && !force {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "hostname '{hostname}' already exists.\n  Use --force to add a second entry, or --overwrite / -o to replace the current one."
            ),
        ));
    } else {
        hosts.append_entry(new_line.clone());
        format!("add '{hostname}'")
    };

    apply_edit(path, &current, &hosts.render(), &action, dry_run)?;
    if !dry_run {
        if duplicate_count > 0 && overwrite {
            println!("✓ Overwritten: {new_line}");
        } else {
            println!("✓ Added: {new_line}");
        }
    }
    Ok(())
}

fn cmd_remove(path: &Path, hostname: &str, dry_run: bool) -> io::Result<()> {
    validate_hostname(hostname)?;
    let current = read_hosts(path)?;
    let mut hosts = HostsFile::parse(&current);
    let removed = hosts.remove_hostname(hostname);

    if removed == 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("hostname '{hostname}' not found in hosts file"),
        ));
    }

    let action = format!("remove {removed} mapping(s) for '{hostname}'");
    apply_edit(path, &current, &hosts.render(), &action, dry_run)?;
    if !dry_run {
        println!("✓ Removed {removed} mapping(s) for '{hostname}'.");
    }
    Ok(())
}

fn cmd_list(path: &Path) -> io::Result<()> {
    let content = read_hosts(path)?;
    let hosts = HostsFile::parse(&content);
    let mut found = false;

    println!("{:<20} HOSTNAME(S)", "IP ADDRESS");
    println!("{}", "-".repeat(50));

    for line in hosts.lines {
        if let Some(entry) = ParsedEntry::parse(&line.text) {
            println!("{:<20} {}", entry.ip, entry.hostnames.join(" "));
            found = true;
        }
    }

    if !found {
        println!("(no entries found)");
    }
    Ok(())
}

fn cmd_undo(path: &Path, dry_run: bool) -> io::Result<()> {
    let current = read_hosts(path)?;
    let backup = backup_path(path)?;
    let previous = fs::read_to_string(&backup).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read backup '{}': {error}", backup.display()),
        )
    })?;
    let action = format!("restore '{}', using '{}'", path.display(), backup.display());

    apply_edit(path, &current, &previous, &action, dry_run)?;
    if !dry_run {
        println!("✓ Restored '{}'.", path.display());
    }
    Ok(())
}

// ── Safe writes and previews ──────────────────────────────────────────────────

fn apply_edit(
    path: &Path,
    current: &str,
    proposed: &str,
    action: &str,
    dry_run: bool,
) -> io::Result<()> {
    if dry_run {
        print_preview(current, proposed, action);
        return Ok(());
    }

    let permissions = fs::metadata(path)?.permissions();
    let backup = backup_path(path)?;
    atomic_write(&backup, current.as_bytes(), Some(permissions.clone()))?;
    atomic_write(path, proposed.as_bytes(), Some(permissions))?;
    println!("Backup: {}", backup.display());
    Ok(())
}

fn atomic_write(
    path: &Path,
    content: &[u8],
    permissions: Option<fs::Permissions>,
) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let mut temporary = NamedTempFile::new_in(parent)?;
    temporary.write_all(content)?;
    temporary.as_file_mut().flush()?;
    if let Some(permissions) = permissions {
        temporary.as_file().set_permissions(permissions)?;
    }
    temporary.as_file().sync_all()?;
    temporary.persist(path).map(|_| ()).map_err(|error| {
        io::Error::new(
            error.error.kind(),
            format!(
                "cannot atomically write '{}': {}",
                path.display(),
                error.error
            ),
        )
    })
}

fn print_preview(current: &str, proposed: &str, action: &str) {
    println!("Dry run — would {action}");

    let before: Vec<&str> = current.lines().collect();
    let after: Vec<&str> = proposed.lines().collect();
    let prefix = before
        .iter()
        .zip(&after)
        .take_while(|(left, right)| left == right)
        .count();
    let suffix = before[prefix..]
        .iter()
        .rev()
        .zip(after[prefix..].iter().rev())
        .take_while(|(left, right)| left == right)
        .count();

    for line in &before[prefix..before.len().saturating_sub(suffix)] {
        println!("- {line}");
    }
    for line in &after[prefix..after.len().saturating_sub(suffix)] {
        println!("+ {line}");
    }
    println!("No files were changed.");
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn validate_hostname(hostname: &str) -> io::Result<()> {
    if hostname.is_empty() || hostname.contains(char::is_whitespace) || hostname.contains('#') {
        return invalid_input("hostname must not be empty or contain whitespace or '#'");
    }
    Ok(())
}

fn invalid_input<T>(message: impl Into<String>) -> io::Result<T> {
    Err(io::Error::new(io::ErrorKind::InvalidInput, message.into()))
}

fn backup_path(path: &Path) -> io::Result<PathBuf> {
    let Some(file_name) = path.file_name() else {
        return invalid_input(format!("'{}' is not a file path", path.display()));
    };
    let mut backup_name = OsString::from(file_name);
    backup_name.push(".hostctl.bak");
    Ok(path.with_file_name(backup_name))
}

fn read_hosts(path: &Path) -> io::Result<String> {
    fs::read_to_string(path).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot read '{}': {error}", path.display()),
        )
    })
}

#[cfg(test)]
mod tests;

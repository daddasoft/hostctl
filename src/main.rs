use clap::{Parser, Subcommand};
use std::{
    fs,
    io::{self, Write},
    net::IpAddr,
    path::PathBuf,
};

const HOSTS_PATH: &str = r"C:\Windows\System32\drivers\etc\hosts";

// ── CLI definition ────────────────────────────────────────────────────────────

#[derive(Parser)]
#[command(
    name = "addhost",
    version,
    about = "Manage the Windows hosts file",
    long_about = None,
)]
struct Cli {
    /// Path to the hosts file (default: C:\\Windows\\System32\\drivers\\etc\\hosts)
    #[arg(long, global = true)]
    hosts: Option<PathBuf>,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a new entry  (e.g. addhost add 127.0.0.1 toto.local)
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
        /// Replace the existing entry instead of adding a new one
        #[arg(short, long)]
        overwrite: bool,
    },

    /// Remove an entry by hostname
    Remove {
        /// Hostname to remove
        hostname: String,
    },

    /// List all non-comment entries
    List,
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() {
    let cli = Cli::parse();
    let hosts_path = cli
        .hosts
        .unwrap_or_else(|| PathBuf::from(HOSTS_PATH));

    let result = match cli.command {
        Command::Add { ip, hostname, comment, force, overwrite } => {
            cmd_add(&hosts_path, &ip, &hostname, comment, force, overwrite)
        }
        Command::Remove { hostname }           => cmd_remove(&hosts_path, &hostname),
        Command::List                          => cmd_list(&hosts_path),
    };

    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

// ── Commands ──────────────────────────────────────────────────────────────────

fn cmd_add(
    path: &PathBuf,
    ip: &str,
    hostname: &str,
    comment: Option<String>,
    force: bool,
    overwrite: bool,
) -> io::Result<()> {
    // --force and --overwrite are mutually exclusive.
    if force && overwrite {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "--force and --overwrite cannot be used together",
        ));
    }

    // Validate the IP address.
    if ip.parse::<IpAddr>().is_err() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{}' is not a valid IP address", ip),
        ));
    }

    // Validate the hostname (basic sanity check).
    if hostname.is_empty() || hostname.contains(char::is_whitespace) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "hostname must not be empty or contain whitespace",
        ));
    }

    let content = read_hosts(path)?;

    // Build the new line.
    let new_line = match &comment {
        Some(c) => format!("{}\t{}\t# {}", ip, hostname, c),
        None    => format!("{}\t{}", ip, hostname),
    };

    // Check for duplicate hostname (case-insensitive).
    let duplicate = content.lines().any(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            return false;
        }
        let parts: Vec<&str> = trimmed.split_whitespace().collect();
        parts.len() >= 2 && parts[1..].iter().any(|h| h.eq_ignore_ascii_case(hostname))
    });

    if duplicate {
        if overwrite {
            // Build new content, replacing every matching line in-place.
            let new_content: String = content
                .lines()
                .map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with('#') || trimmed.is_empty() {
                        return line.to_string();
                    }
                    let parts: Vec<&str> = trimmed.split_whitespace().collect();
                    if parts.len() >= 2
                        && parts[1..].iter().any(|h| h.eq_ignore_ascii_case(hostname))
                    {
                        return new_line.clone();
                    }
                    line.to_string()
                })
                .collect::<Vec<_>>()
                .join("\n")
                + "\n";
            fs::write(path, new_content)?;
            println!("✓ Overwritten:  {}", new_line);
            return Ok(());
        } else if force {
            // Fall through and append anyway.
        } else {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!(
                    "hostname '{}' already exists.\n  Use --force to add a second entry, or --overwrite / -o to replace the current one.",
                    hostname
                ),
            ));
        }
    }

    // Append the new line.
    let mut file = fs::OpenOptions::new().append(true).open(path)?;
    if !content.is_empty() && !content.ends_with('\n') {
        writeln!(file)?;
    }
    writeln!(file, "{}", new_line)?;

    println!("✓ Added:  {}", new_line);
    Ok(())
}

fn cmd_remove(path: &PathBuf, hostname: &str) -> io::Result<()> {
    let content = read_hosts(path)?;
    let mut removed = 0usize;

    let kept: Vec<&str> = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            if trimmed.starts_with('#') || trimmed.is_empty() {
                return true; // always keep comments & blanks
            }
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            let matches = parts.len() >= 2
                && parts[1..].iter().any(|h| h.eq_ignore_ascii_case(hostname));
            if matches {
                removed += 1;
            }
            !matches
        })
        .collect();

    if removed == 0 {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("hostname '{}' not found in hosts file", hostname),
        ));
    }

    let new_content = kept.join("\n") + "\n";
    fs::write(path, new_content)?;
    println!("✓ Removed {} entry/entries for '{}'.", removed, hostname);
    Ok(())
}

fn cmd_list(path: &PathBuf) -> io::Result<()> {
    let content = read_hosts(path)?;
    let mut found = false;

    println!("{:<20} {}", "IP ADDRESS", "HOSTNAME(S)");
    println!("{}", "-".repeat(50));

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        // Strip inline comment before printing.
        let data = trimmed.splitn(2, '#').next().unwrap_or("").trim();
        let parts: Vec<&str> = data.split_whitespace().collect();
        if parts.len() >= 2 {
            println!("{:<20} {}", parts[0], parts[1..].join(" "));
            found = true;
        }
    }

    if !found {
        println!("(no entries found)");
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn read_hosts(path: &PathBuf) -> io::Result<String> {
    fs::read_to_string(path).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("cannot read '{}': {}", path.display(), e),
        )
    })
}

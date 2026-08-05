use clap::{Parser, Subcommand};
use hostctl::{
    BackupInfo, MutationResult, add_entry, create_backup, default_hosts_path, list_backups,
    list_entries, remove_entry, restore_backup,
};
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "hostctl",
    version,
    about = "Safely manage the system hosts file",
    long_about = None
)]
struct Cli {
    /// Override the system hosts file path
    #[arg(long, global = true)]
    hosts: Option<PathBuf>,

    /// Preview the exact proposed contents without writing or creating a backup
    #[arg(long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a new hostname mapping
    Add {
        /// IPv4 or IPv6 address
        ip: String,
        /// Hostname to map
        hostname: String,
        /// Optional inline comment
        #[arg(short, long)]
        comment: Option<String>,
        /// Permit another mapping for an existing hostname
        #[arg(long)]
        force: bool,
        /// Change an existing hostname mapping without discarding its aliases
        #[arg(short, long)]
        overwrite: bool,
    },

    /// Remove a hostname without removing other aliases on the same line
    Remove {
        /// Hostname to remove
        hostname: String,
    },

    /// List active entries
    List,

    /// Create and inspect checksum-protected backups
    Backup {
        #[command(subcommand)]
        command: Option<BackupCommand>,
    },

    /// Restore a verified backup, or the latest backup when no ID is supplied
    Restore {
        /// Backup ID shown by 'hostctl backup list', or 'latest'
        backup: Option<String>,
    },
}

#[derive(Subcommand)]
enum BackupCommand {
    /// Create a backup immediately
    Create,
    /// List backups and verify their checksums
    List,
}

fn main() {
    let cli = Cli::parse();
    let path = cli.hosts.unwrap_or_else(default_hosts_path);

    let result = match cli.command {
        Command::Add {
            ip,
            hostname,
            comment,
            force,
            overwrite,
        } => add_entry(
            &path,
            &ip,
            &hostname,
            comment.as_deref(),
            force,
            overwrite,
            cli.dry_run,
        )
        .and_then(|result| {
            finish_mutation(&path, &result, cli.dry_run)?;
            if !cli.dry_run {
                let action = if overwrite { "Overwritten" } else { "Added" };
                println!("{action}: {ip}\t{hostname}");
            }
            Ok(())
        }),
        Command::Remove { hostname } => {
            remove_entry(&path, &hostname, cli.dry_run).and_then(|(result, removed)| {
                finish_mutation(&path, &result, cli.dry_run)?;
                if !cli.dry_run {
                    println!("Removed {removed} mapping(s) for '{hostname}'.");
                }
                Ok(())
            })
        }
        Command::List => cmd_list(&path),
        Command::Backup { command } => match command.unwrap_or(BackupCommand::Create) {
            BackupCommand::Create => create_backup(&path).map(|backup| {
                println!("Backup created: {}", backup.id);
                println!("SHA-256: {}", backup.checksum);
            }),
            BackupCommand::List => cmd_backup_list(&path),
        },
        Command::Restore { backup } => restore_backup(&path, backup.as_deref(), cli.dry_run)
            .and_then(|result| {
                finish_mutation(&path, &result, cli.dry_run)?;
                if !cli.dry_run {
                    println!("Restored verified backup.");
                }
                Ok(())
            }),
    };

    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn cmd_list(path: &Path) -> io::Result<()> {
    let entries = list_entries(path)?;
    println!("{:<39} HOSTNAME(S)", "IP ADDRESS");
    println!("{}", "-".repeat(70));
    if entries.is_empty() {
        println!("(no entries found)");
        return Ok(());
    }
    for entry in entries {
        println!("{:<39} {}", entry.ip, entry.hostnames.join(" "));
    }
    Ok(())
}

fn cmd_backup_list(path: &Path) -> io::Result<()> {
    let backups = list_backups(path)?;
    if backups.is_empty() {
        println!("(no backups found)");
        return Ok(());
    }
    println!("{:<48} {:<8} SHA-256", "BACKUP ID", "STATUS");
    println!("{}", "-".repeat(125));
    for backup in backups {
        let status = if backup.valid { "valid" } else { "INVALID" };
        println!("{:<48} {:<8} {}", backup.id, status, backup.checksum);
    }
    Ok(())
}

fn finish_mutation(path: &Path, result: &MutationResult, dry_run: bool) -> io::Result<()> {
    if dry_run {
        print_preview(path, result)?;
    } else if let Some(BackupInfo { id, .. }) = &result.backup {
        println!("Backup: {id}");
    }
    if !dry_run && !result.atomic {
        eprintln!(
            "warning: the parent directory is protected; used a locked in-place write with a verified user backup"
        );
    }
    Ok(())
}

fn print_preview(path: &Path, result: &MutationResult) -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    writeln!(stdout, "Dry run: '{}' was not changed.", path.display())?;
    writeln!(stdout, "--- current ({} bytes)", result.before.len())?;
    stdout.write_all(&result.before)?;
    if !result.before.ends_with(b"\n") {
        writeln!(stdout, "\n\\ No newline at end of file")?;
    }
    writeln!(stdout, "+++ proposed ({} bytes)", result.after.len())?;
    stdout.write_all(&result.after)?;
    if !result.after.ends_with(b"\n") {
        writeln!(stdout, "\n\\ No newline at end of file")?;
    }
    Ok(())
}

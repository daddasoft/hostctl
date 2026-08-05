use clap::{CommandFactory, Parser, Subcommand, ValueEnum};
use clap_complete::{Shell, generate};
use hostctl::{
    BackupInfo, DataFormat, Entry, MutationResult, add_entries, check_update,
    config::{
        ManagedEntry, Profile, ProjectConfig, apply_config, default_config_path, init_config,
        load_config, load_or_default, save_config,
    },
    create_backup, default_hosts_path, doctor, flush_dns, import_entries, list_all_entries,
    list_backups, parse_entries, remove_by_ip, remove_entry, remove_entry_from_ip, resolve,
    restore_backup, self_uninstall, self_update, serialize_entries, set_entry_enabled,
    update_entry,
};
use serde::Serialize;
use std::{
    fs,
    io::{self, Read, Write},
    net::IpAddr,
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

    /// Override the project configuration path
    #[arg(long, global = true)]
    config: Option<PathBuf>,

    /// Preview changes without writing or creating a backup
    #[arg(long, global = true)]
    dry_run: bool,

    /// Select human or machine-readable output
    #[arg(long = "format", global = true, value_enum, default_value_t = OutputFormat::Table)]
    output_format: OutputFormat,

    /// Suppress successful command output
    #[arg(short, long, global = true)]
    quiet: bool,

    /// Print operational details to stderr
    #[arg(short, long, global = true)]
    verbose: bool,

    /// Disable colored output (reserved for scripting compatibility)
    #[arg(long, global = true)]
    no_color: bool,

    /// Flush the platform DNS cache after a successful mutation
    #[arg(long, global = true)]
    flush_dns: bool,

    #[command(subcommand)]
    command: Command,
}

#[derive(Clone, Copy, Debug, ValueEnum, PartialEq, Eq)]
enum OutputFormat {
    Table,
    Json,
    Yaml,
    Plain,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum SortField {
    Ip,
    Hostname,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum FileFormat {
    Hosts,
    Json,
    Yaml,
    Toml,
}

impl From<FileFormat> for DataFormat {
    fn from(value: FileFormat) -> Self {
        match value {
            FileFormat::Hosts => Self::Hosts,
            FileFormat::Json => Self::Json,
            FileFormat::Yaml => Self::Yaml,
            FileFormat::Toml => Self::Toml,
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ImportMode {
    Merge,
    Replace,
}

#[derive(Subcommand)]
enum Command {
    /// Add one or more aliases for an IP address
    Add {
        ip: String,
        #[arg(required = true)]
        hostnames: Vec<String>,
        #[arg(short, long)]
        comment: Option<String>,
        #[arg(long)]
        force: bool,
        #[arg(short, long)]
        overwrite: bool,
    },
    /// Remove mappings by hostname or IP address
    Remove {
        hostname: Option<String>,
        #[arg(long, conflicts_with = "hostname")]
        ip: Option<String>,
        /// Remove this hostname only from the selected IP
        #[arg(long, requires = "hostname")]
        from_ip: Option<String>,
    },
    /// Show mappings for one hostname
    Get { hostname: String },
    /// Search IPs, hostnames, and comments
    Search { pattern: String },
    /// Update or rename a hostname
    Update {
        hostname: String,
        #[arg(long)]
        ip: Option<String>,
        #[arg(long)]
        rename: Option<String>,
        #[arg(short, long)]
        comment: Option<String>,
    },
    /// Comment out a hostname mapping
    Disable { hostname: String },
    /// Reactivate a hostctl-disabled mapping
    Enable { hostname: String },
    /// Show whether a hostname is active, disabled, duplicated, or missing
    Status { hostname: String },
    /// Idempotently ensure that a mapping is present or absent
    Ensure {
        #[command(subcommand)]
        command: EnsureCommand,
    },
    /// List and filter entries
    List {
        #[arg(long)]
        hostname: Option<String>,
        #[arg(long)]
        ip: Option<String>,
        #[arg(long, conflicts_with = "ipv6")]
        ipv4: bool,
        #[arg(long)]
        ipv6: bool,
        #[arg(long, value_enum)]
        sort: Option<SortField>,
        #[arg(long)]
        include_disabled: bool,
    },
    /// Remove entries from the hostctl-managed block
    Clear,
    /// Create and inspect checksum-protected backups
    Backup {
        #[command(subcommand)]
        command: Option<BackupCommand>,
    },
    /// Restore a verified backup
    Restore { backup: Option<String> },
    /// Export entries to stdout or a file
    Export {
        #[arg(short, long = "file-format", value_enum, default_value_t = FileFormat::Hosts)]
        format: FileFormat,
        #[arg(short, long)]
        output: Option<PathBuf>,
        #[arg(long)]
        include_disabled: bool,
    },
    /// Import entries from a file or stdin
    Import {
        /// Input path, or '-' for stdin
        input: PathBuf,
        #[arg(short, long = "file-format", value_enum)]
        format: Option<FileFormat>,
        #[arg(long, value_enum, default_value_t = ImportMode::Merge)]
        mode: ImportMode,
        /// Confirm destructive replace mode
        #[arg(long)]
        yes: bool,
    },
    /// Manage the versioned .hostctl.toml configuration
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// Manage named entry groups
    Group {
        #[command(subcommand)]
        command: GroupCommand,
    },
    /// Manage environment profiles
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Diagnose file, syntax, duplicate, and backup state
    Doctor,
    /// Flush the operating system DNS cache
    FlushDns,
    /// Compare hosts-file and system resolution
    Resolve { hostname: String },
    /// Generate shell completion code
    Completion { shell: CompletionShell },
    /// Generate the hostctl manual page
    Man {
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Check for or install the latest GitHub release
    SelfUpdate {
        /// Check without installing
        #[arg(long)]
        check: bool,
    },
    /// Remove the installed binary and PATH entry
    SelfUninstall,
}

impl Command {
    fn name(&self) -> &'static str {
        match self {
            Self::Add { .. } => "add",
            Self::Remove { .. } => "remove",
            Self::Get { .. } => "get",
            Self::Search { .. } => "search",
            Self::Update { .. } => "update",
            Self::Disable { .. } => "disable",
            Self::Enable { .. } => "enable",
            Self::Status { .. } => "status",
            Self::Ensure { .. } => "ensure",
            Self::List { .. } => "list",
            Self::Clear => "clear",
            Self::Backup { .. } => "backup",
            Self::Restore { .. } => "restore",
            Self::Export { .. } => "export",
            Self::Import { .. } => "import",
            Self::Config { .. } => "config",
            Self::Group { .. } => "group",
            Self::Profile { .. } => "profile",
            Self::Doctor => "doctor",
            Self::FlushDns => "flush-dns",
            Self::Resolve { .. } => "resolve",
            Self::Completion { .. } => "completion",
            Self::Man { .. } => "man",
            Self::SelfUpdate { .. } => "self-update",
            Self::SelfUninstall => "self-uninstall",
        }
    }
}

#[derive(Subcommand)]
enum EnsureCommand {
    Present {
        ip: String,
        #[arg(required = true)]
        hostnames: Vec<String>,
        #[arg(short, long)]
        comment: Option<String>,
    },
    Absent {
        hostname: String,
    },
}

#[derive(Subcommand)]
enum BackupCommand {
    Create,
    List,
}

#[derive(Subcommand)]
enum ConfigCommand {
    Init,
    Validate,
    Apply,
    Diff,
    Path,
}

#[derive(Subcommand)]
enum GroupCommand {
    List,
    Show {
        name: String,
    },
    Add {
        name: String,
        ip: String,
        #[arg(required = true)]
        hostnames: Vec<String>,
        #[arg(short, long)]
        comment: Option<String>,
    },
    Enable {
        name: String,
    },
    Disable {
        name: String,
    },
    Remove {
        name: String,
    },
}

#[derive(Subcommand)]
enum ProfileCommand {
    List,
    Show {
        name: String,
    },
    Create {
        name: String,
        #[arg(required = true)]
        groups: Vec<String>,
    },
    Activate {
        name: String,
    },
    Remove {
        name: String,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Zsh,
    Fish,
    Powershell,
    Elvish,
}

impl From<CompletionShell> for Shell {
    fn from(value: CompletionShell) -> Self {
        match value {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Zsh => Self::Zsh,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::Powershell => Self::PowerShell,
            CompletionShell::Elvish => Self::Elvish,
        }
    }
}

struct Context {
    hosts: PathBuf,
    config: PathBuf,
    dry_run: bool,
    format: OutputFormat,
    quiet: bool,
    verbose: bool,
    flush_dns: bool,
}

fn main() {
    let cli = Cli::parse();
    let config_path = cli
        .config
        .clone()
        .map(Ok)
        .unwrap_or_else(default_config_path);
    let result = config_path.and_then(|config| {
        let context = Context {
            hosts: cli.hosts.clone().unwrap_or_else(default_hosts_path),
            config,
            dry_run: cli.dry_run,
            format: cli.output_format,
            quiet: cli.quiet,
            verbose: cli.verbose,
            flush_dns: cli.flush_dns,
        };
        if context.verbose {
            eprintln!("hosts: {}", context.hosts.display());
            eprintln!("config: {}", context.config.display());
        }
        if !context.quiet {
            eprintln!("info: executing command: {}", cli.command.name());
        }
        run(&context, cli.command)
    });

    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(exit_code(&error));
    }
}

fn run(context: &Context, command: Command) -> io::Result<()> {
    match command {
        Command::Add {
            ip,
            hostnames,
            comment,
            force,
            overwrite,
        } => {
            let result = add_entries(
                &context.hosts,
                &ip,
                &hostnames,
                comment.as_deref(),
                force,
                overwrite,
                context.dry_run,
            )?;
            finish_mutation(context, &result, "mapping added")
        }
        Command::Remove {
            hostname,
            ip,
            from_ip,
        } => {
            if let Some(hostname) = hostname {
                let (result, count) = if let Some(from_ip) = from_ip {
                    remove_entry_from_ip(&context.hosts, &hostname, &from_ip, context.dry_run)?
                } else {
                    remove_entry(&context.hosts, &hostname, context.dry_run)?
                };
                finish_mutation(context, &result, &format!("removed {count} mapping(s)"))
            } else if let Some(ip) = ip {
                let (result, count) = remove_by_ip(&context.hosts, &ip, context.dry_run)?;
                finish_mutation(context, &result, &format!("removed {count} hostname(s)"))
            } else {
                Err(invalid("remove requires a hostname or --ip"))
            }
        }
        Command::Get { hostname } => {
            let entries = matching_entries(&context.hosts, &hostname, true)?;
            if entries.is_empty() {
                return Err(not_found(format!("hostname '{hostname}' not found")));
            }
            print_entries(&entries, context.format)
        }
        Command::Search { pattern } => {
            let pattern = pattern.to_ascii_lowercase();
            let entries = list_all_entries(&context.hosts)?
                .into_iter()
                .filter(|entry| {
                    entry.ip.to_ascii_lowercase().contains(&pattern)
                        || entry
                            .hostnames
                            .iter()
                            .any(|hostname| hostname.to_ascii_lowercase().contains(&pattern))
                        || entry
                            .comment
                            .as_deref()
                            .is_some_and(|comment| comment.to_ascii_lowercase().contains(&pattern))
                })
                .collect::<Vec<_>>();
            print_entries(&entries, context.format)
        }
        Command::Update {
            hostname,
            ip,
            rename,
            comment,
        } => {
            let result = update_entry(
                &context.hosts,
                &hostname,
                ip.as_deref(),
                rename.as_deref(),
                comment.as_deref(),
                context.dry_run,
            )?;
            finish_mutation(context, &result, "mapping updated")
        }
        Command::Disable { hostname } => {
            let result = set_entry_enabled(&context.hosts, &hostname, false, context.dry_run)?;
            finish_mutation(context, &result, "mapping disabled")
        }
        Command::Enable { hostname } => {
            let result = set_entry_enabled(&context.hosts, &hostname, true, context.dry_run)?;
            finish_mutation(context, &result, "mapping enabled")
        }
        Command::Status { hostname } => print_status(context, &hostname),
        Command::Ensure { command } => match command {
            EnsureCommand::Present {
                ip,
                hostnames,
                comment,
            } => {
                let result = add_entries(
                    &context.hosts,
                    &ip,
                    &hostnames,
                    comment.as_deref(),
                    false,
                    true,
                    context.dry_run,
                )?;
                finish_mutation(context, &result, "mapping is present")
            }
            EnsureCommand::Absent { hostname } => {
                if matching_entries(&context.hosts, &hostname, true)?.is_empty() {
                    print_message(context, "mapping is absent");
                    return Ok(());
                }
                let (result, _) = remove_entry(&context.hosts, &hostname, context.dry_run)?;
                finish_mutation(context, &result, "mapping is absent")
            }
        },
        Command::List {
            hostname,
            ip,
            ipv4,
            ipv6,
            sort,
            include_disabled,
        } => {
            let mut entries = if include_disabled {
                list_all_entries(&context.hosts)?
            } else {
                hostctl::list_entries(&context.hosts)?
            };
            entries.retain(|entry| {
                hostname.as_ref().is_none_or(|filter| {
                    entry
                        .hostnames
                        .iter()
                        .any(|value| value.eq_ignore_ascii_case(filter))
                }) && ip.as_ref().is_none_or(|filter| entry.ip == *filter)
                    && (!ipv4
                        || entry
                            .ip
                            .parse::<IpAddr>()
                            .is_ok_and(|value| value.is_ipv4()))
                    && (!ipv6
                        || entry
                            .ip
                            .parse::<IpAddr>()
                            .is_ok_and(|value| value.is_ipv6()))
            });
            match sort {
                Some(SortField::Ip) => entries.sort_by(|a, b| a.ip.cmp(&b.ip)),
                Some(SortField::Hostname) => entries.sort_by(|a, b| {
                    a.hostnames
                        .first()
                        .cmp(&b.hostnames.first())
                        .then(a.ip.cmp(&b.ip))
                }),
                None => {}
            }
            print_entries(&entries, context.format)
        }
        Command::Clear => {
            let config = ProjectConfig::default();
            let result = apply_config(&context.hosts, &config, context.dry_run)?;
            finish_mutation(context, &result, "managed entries cleared")
        }
        Command::Backup { command } => match command.unwrap_or(BackupCommand::Create) {
            BackupCommand::Create => {
                let backup = create_backup(&context.hosts)?;
                if !context.quiet {
                    println!("{}\t{}", backup.id, backup.checksum);
                }
                Ok(())
            }
            BackupCommand::List => print_backups(&list_backups(&context.hosts)?, context.format),
        },
        Command::Restore { backup } => {
            let result = restore_backup(&context.hosts, backup.as_deref(), context.dry_run)?;
            finish_mutation(context, &result, "backup restored")
        }
        Command::Export {
            format,
            output,
            include_disabled,
        } => {
            let entries = if include_disabled {
                list_all_entries(&context.hosts)?
            } else {
                hostctl::list_entries(&context.hosts)?
            };
            let serialized = serialize_entries(&entries, format.into())?;
            write_output(output.as_deref(), serialized.as_bytes())
        }
        Command::Import {
            input,
            format,
            mode,
            yes,
        } => {
            if matches!(mode, ImportMode::Replace) && !yes && !context.dry_run {
                return Err(invalid("replace mode requires --yes or --dry-run"));
            }
            let content = read_input(&input)?;
            let format = format.unwrap_or_else(|| infer_format(&input));
            let entries = parse_entries(&content, format.into())?;
            let result = import_entries(
                &context.hosts,
                &entries,
                matches!(mode, ImportMode::Replace),
                context.dry_run,
            )?;
            finish_mutation(context, &result, "entries imported")
        }
        Command::Config { command } => run_config(context, command),
        Command::Group { command } => run_group(context, command),
        Command::Profile { command } => run_profile(context, command),
        Command::Doctor => {
            let report = doctor(&context.hosts)?;
            match context.format {
                OutputFormat::Json | OutputFormat::Yaml => {
                    print_serialized(&report, context.format)
                }
                _ => {
                    println!("Hosts file: {}", report.hosts_path.display());
                    println!("Readable: {}", report.readable);
                    println!("Writable: {}", report.writable);
                    println!("Entries: {}", report.entry_count);
                    println!("Disabled: {}", report.disabled_count);
                    println!("Malformed lines: {}", report.malformed_lines);
                    println!("Duplicates: {}", report.duplicate_hostnames.join(", "));
                    println!("Healthy: {}", report.healthy);
                    Ok(())
                }
            }
        }
        Command::FlushDns => {
            flush_dns()?;
            print_message(context, "DNS cache flushed");
            Ok(())
        }
        Command::Resolve { hostname } => {
            let resolution = resolve(&context.hosts, &hostname)?;
            match context.format {
                OutputFormat::Json | OutputFormat::Yaml => {
                    print_serialized(&resolution, context.format)
                }
                _ => {
                    println!("Hostname: {}", resolution.hostname);
                    println!(
                        "Hosts file: {}",
                        resolution
                            .hosts_file
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    println!(
                        "System: {}",
                        resolution
                            .system
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    println!("Conflict: {}", resolution.conflict);
                    Ok(())
                }
            }
        }
        Command::Completion { shell } => {
            let mut command = Cli::command();
            generate(
                Shell::from(shell),
                &mut command,
                "hostctl",
                &mut io::stdout(),
            );
            Ok(())
        }
        Command::Man { output } => {
            let man = clap_mangen::Man::new(Cli::command());
            let mut buffer = Vec::new();
            man.render(&mut buffer)?;
            write_output(output.as_deref(), &buffer)
        }
        Command::SelfUpdate { check } => {
            if check {
                let status = check_update()?;
                match context.format {
                    OutputFormat::Json | OutputFormat::Yaml => {
                        print_serialized(&status, context.format)
                    }
                    _ => {
                        println!("Current: {}", status.current);
                        println!("Latest: {}", status.latest);
                        println!("Update available: {}", status.update_available);
                        Ok(())
                    }
                }
            } else {
                self_update()?;
                print_message(context, "hostctl updated");
                Ok(())
            }
        }
        Command::SelfUninstall => {
            self_uninstall()?;
            print_message(context, "hostctl uninstall started");
            Ok(())
        }
    }
}

fn run_config(context: &Context, command: ConfigCommand) -> io::Result<()> {
    match command {
        ConfigCommand::Init => {
            init_config(&context.config)?;
            print_message(context, "configuration initialized");
            Ok(())
        }
        ConfigCommand::Validate => {
            load_config(&context.config)?;
            print_message(context, "configuration is valid");
            Ok(())
        }
        ConfigCommand::Apply => {
            let config = load_config(&context.config)?;
            let result = apply_config(&context.hosts, &config, context.dry_run)?;
            finish_mutation(context, &result, "configuration applied")
        }
        ConfigCommand::Diff => {
            let config = load_config(&context.config)?;
            let result = apply_config(&context.hosts, &config, true)?;
            print_mutation_preview(context, &result)
        }
        ConfigCommand::Path => {
            println!("{}", context.config.display());
            Ok(())
        }
    }
}

fn run_group(context: &Context, command: GroupCommand) -> io::Result<()> {
    let mut config = load_or_default(&context.config)?;
    match command {
        GroupCommand::List => {
            let rows = config
                .groups
                .iter()
                .map(|(name, group)| (name.clone(), group.enabled, group.entries.len()))
                .collect::<Vec<_>>();
            if matches!(context.format, OutputFormat::Json | OutputFormat::Yaml) {
                print_serialized(&config.groups, context.format)
            } else {
                println!("{:<24} {:<8} ENTRIES", "GROUP", "ENABLED");
                for (name, enabled, count) in rows {
                    println!("{name:<24} {enabled:<8} {count}");
                }
                Ok(())
            }
        }
        GroupCommand::Show { name } => {
            let group = config
                .groups
                .get(&name)
                .ok_or_else(|| not_found(format!("group '{name}' not found")))?;
            print_serialized(group, context.format)
        }
        GroupCommand::Add {
            name,
            ip,
            hostnames,
            comment,
        } => {
            config
                .groups
                .entry(name)
                .or_default()
                .entries
                .push(ManagedEntry {
                    ip,
                    hostnames,
                    comment,
                });
            apply_and_save(context, &config, "group entry added")
        }
        GroupCommand::Enable { name } => {
            let group = config
                .groups
                .get_mut(&name)
                .ok_or_else(|| not_found(format!("group '{name}' not found")))?;
            group.enabled = true;
            apply_and_save(context, &config, "group enabled")
        }
        GroupCommand::Disable { name } => {
            let group = config
                .groups
                .get_mut(&name)
                .ok_or_else(|| not_found(format!("group '{name}' not found")))?;
            group.enabled = false;
            apply_and_save(context, &config, "group disabled")
        }
        GroupCommand::Remove { name } => {
            if config.groups.remove(&name).is_none() {
                return Err(not_found(format!("group '{name}' not found")));
            }
            for profile in config.profiles.values_mut() {
                profile.groups.retain(|group| group != &name);
            }
            apply_and_save(context, &config, "group removed")
        }
    }
}

fn run_profile(context: &Context, command: ProfileCommand) -> io::Result<()> {
    let mut config = load_or_default(&context.config)?;
    match command {
        ProfileCommand::List => print_serialized(&config.profiles, context.format),
        ProfileCommand::Show { name } => {
            let profile = config
                .profiles
                .get(&name)
                .ok_or_else(|| not_found(format!("profile '{name}' not found")))?;
            print_serialized(profile, context.format)
        }
        ProfileCommand::Create { name, groups } => {
            if config.profiles.contains_key(&name) {
                return Err(io::Error::new(
                    io::ErrorKind::AlreadyExists,
                    format!("profile '{name}' already exists"),
                ));
            }
            config.profiles.insert(name, Profile { groups });
            config.validate()?;
            if !context.dry_run {
                save_config(&context.config, &config)?;
            }
            print_message(context, "profile created");
            Ok(())
        }
        ProfileCommand::Activate { name } => {
            config.activate_profile(&name)?;
            apply_and_save(context, &config, "profile activated")
        }
        ProfileCommand::Remove { name } => {
            if config.profiles.remove(&name).is_none() {
                return Err(not_found(format!("profile '{name}' not found")));
            }
            if config.active_profile.as_deref() == Some(&name) {
                config.active_profile = None;
            }
            apply_and_save(context, &config, "profile removed")
        }
    }
}

fn apply_and_save(context: &Context, config: &ProjectConfig, message: &str) -> io::Result<()> {
    config.validate()?;
    let result = apply_config(&context.hosts, config, context.dry_run)?;
    if !context.dry_run {
        save_config(&context.config, config)?;
    }
    finish_mutation(context, &result, message)
}

fn matching_entries(path: &Path, hostname: &str, disabled: bool) -> io::Result<Vec<Entry>> {
    Ok(list_all_entries(path)?
        .into_iter()
        .filter(|entry| {
            (disabled || !entry.disabled)
                && entry
                    .hostnames
                    .iter()
                    .any(|value| value.eq_ignore_ascii_case(hostname))
        })
        .collect())
}

fn print_status(context: &Context, hostname: &str) -> io::Result<()> {
    let entries = matching_entries(&context.hosts, hostname, true)?;
    #[derive(Serialize)]
    struct Status<'a> {
        hostname: &'a str,
        status: &'a str,
        count: usize,
        entries: &'a [Entry],
    }
    let active = entries.iter().filter(|entry| !entry.disabled).count();
    let disabled = entries.iter().filter(|entry| entry.disabled).count();
    let status = if entries.is_empty() {
        "missing"
    } else if active > 1 || disabled > 1 || (active > 0 && disabled > 0) {
        "duplicated"
    } else if active == 1 {
        "active"
    } else {
        "disabled"
    };
    let value = Status {
        hostname,
        status,
        count: entries.len(),
        entries: &entries,
    };
    match context.format {
        OutputFormat::Json | OutputFormat::Yaml => print_serialized(&value, context.format),
        _ => {
            println!("{hostname}: {status}");
            Ok(())
        }
    }
}

fn print_entries(entries: &[Entry], format: OutputFormat) -> io::Result<()> {
    match format {
        OutputFormat::Json | OutputFormat::Yaml => print_serialized(entries, format),
        OutputFormat::Plain => {
            print!("{}", serialize_entries(entries, DataFormat::Hosts)?);
            Ok(())
        }
        OutputFormat::Table => {
            println!("{:<39} {:<10} HOSTNAME(S)", "IP ADDRESS", "STATUS");
            println!("{}", "-".repeat(80));
            if entries.is_empty() {
                println!("(no entries found)");
            }
            for entry in entries {
                println!(
                    "{:<39} {:<10} {}",
                    entry.ip,
                    if entry.disabled { "disabled" } else { "active" },
                    entry.hostnames.join(" ")
                );
            }
            Ok(())
        }
    }
}

fn print_backups(backups: &[BackupInfo], format: OutputFormat) -> io::Result<()> {
    match format {
        OutputFormat::Json | OutputFormat::Yaml => {
            #[derive(Serialize)]
            struct BackupOutput<'a> {
                id: &'a str,
                path: &'a Path,
                checksum: &'a str,
                valid: bool,
            }
            let values = backups
                .iter()
                .map(|backup| BackupOutput {
                    id: &backup.id,
                    path: &backup.path,
                    checksum: &backup.checksum,
                    valid: backup.valid,
                })
                .collect::<Vec<_>>();
            print_serialized(&values, format)
        }
        _ => {
            println!("{:<48} {:<8} SHA-256", "BACKUP ID", "STATUS");
            for backup in backups {
                println!(
                    "{:<48} {:<8} {}",
                    backup.id,
                    if backup.valid { "valid" } else { "INVALID" },
                    backup.checksum
                );
            }
            Ok(())
        }
    }
}

fn print_serialized<T: Serialize + ?Sized>(value: &T, format: OutputFormat) -> io::Result<()> {
    let output = match format {
        OutputFormat::Yaml => serde_yaml::to_string(value)
            .map_err(|error| io::Error::other(format!("cannot serialize YAML: {error}")))?,
        OutputFormat::Json => serde_json::to_string_pretty(value)
            .map_err(|error| io::Error::other(format!("cannot serialize JSON: {error}")))?,
        OutputFormat::Table | OutputFormat::Plain => serde_json::to_string_pretty(value)
            .map_err(|error| io::Error::other(format!("cannot serialize output: {error}")))?,
    };
    println!("{output}");
    Ok(())
}

fn finish_mutation(context: &Context, result: &MutationResult, message: &str) -> io::Result<()> {
    if context.dry_run {
        print_mutation_preview(context, result)?;
    } else if !context.quiet {
        println!("{message}");
        if let Some(backup) = &result.backup
            && context.verbose
        {
            eprintln!("backup path: {}", backup.path.display());
        }
    }
    if !context.dry_run && context.flush_dns && result.before != result.after {
        flush_dns()?;
        if context.verbose {
            eprintln!("DNS cache flushed");
        }
    }
    Ok(())
}

fn print_mutation_preview(context: &Context, result: &MutationResult) -> io::Result<()> {
    #[derive(Serialize)]
    struct Preview<'a> {
        path: &'a Path,
        changed: bool,
        current: String,
        proposed: String,
    }
    match context.format {
        OutputFormat::Json | OutputFormat::Yaml => print_serialized(
            &Preview {
                path: &context.hosts,
                changed: result.before != result.after,
                current: String::from_utf8_lossy(&result.before).into_owned(),
                proposed: String::from_utf8_lossy(&result.after).into_owned(),
            },
            context.format,
        ),
        OutputFormat::Table | OutputFormat::Plain => print_preview(&context.hosts, result),
    }
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

fn read_input(path: &Path) -> io::Result<String> {
    if path == Path::new("-") {
        let mut content = String::new();
        io::stdin().read_to_string(&mut content)?;
        Ok(content)
    } else {
        fs::read_to_string(path).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot read '{}': {error}", path.display()),
            )
        })
    }
}

fn write_output(path: Option<&Path>, content: &[u8]) -> io::Result<()> {
    match path {
        None => io::stdout().write_all(content),
        Some(path) => fs::write(path, content).map_err(|error| {
            io::Error::new(
                error.kind(),
                format!("cannot write '{}': {error}", path.display()),
            )
        }),
    }
}

fn infer_format(path: &Path) -> FileFormat {
    match path.extension().and_then(|extension| extension.to_str()) {
        Some("json") => FileFormat::Json,
        Some("yaml" | "yml") => FileFormat::Yaml,
        Some("toml") => FileFormat::Toml,
        _ => FileFormat::Hosts,
    }
}

fn print_message(context: &Context, message: &str) {
    if !context.quiet {
        println!("{message}");
    }
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message.into())
}

fn not_found(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, message.into())
}

fn exit_code(error: &io::Error) -> i32 {
    match error.kind() {
        io::ErrorKind::InvalidInput => 2,
        io::ErrorKind::NotFound => 3,
        io::ErrorKind::AlreadyExists => 4,
        io::ErrorKind::PermissionDenied => 5,
        io::ErrorKind::WouldBlock => 6,
        io::ErrorKind::InvalidData => 7,
        _ => 1,
    }
}

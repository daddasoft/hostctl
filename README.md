# hostctl

A cross-platform CLI to manage your system **hosts file** — add, remove, and list entries with safety guards against duplicates.

[![CI](https://github.com/daddasoft/hostctl/actions/workflows/ci.yml/badge.svg)](https://github.com/daddasoft/hostctl/actions/workflows/ci.yml)
[![Release](https://github.com/daddasoft/hostctl/actions/workflows/release.yml/badge.svg)](https://github.com/daddasoft/hostctl/releases)

---

## Installation

### Linux & macOS — one-liner

```bash
curl -fsSL https://raw.githubusercontent.com/daddasoft/hostctl/main/install.sh | bash
```

### Windows — one-liner (PowerShell)

```powershell
irm https://raw.githubusercontent.com/daddasoft/hostctl/main/install.ps1 | iex
```

Installs to `~\.hostctl\bin` and adds it to your **user PATH** automatically.  
Override the install directory:

```powershell
$env:HOSTCTL_INSTALL_DIR = "C:\Tools"
irm https://raw.githubusercontent.com/daddasoft/hostctl/main/install.ps1 | iex
```

### Ansible

You can automate the installation across your fleet using Ansible.

**Linux & macOS:**
```yaml
- name: Install hostctl (Linux/macOS)
  ansible.builtin.shell: curl -fsSL https://raw.githubusercontent.com/daddasoft/hostctl/main/install.sh | bash
  args:
    creates: /usr/local/bin/hostctl
```

**Windows:**
```yaml
- name: Install hostctl (Windows)
  ansible.windows.win_shell: irm https://raw.githubusercontent.com/daddasoft/hostctl/main/install.ps1 | iex
  args:
    creates: '%USERPROFILE%\.hostctl\bin\hostctl.exe'
```

### SaltStack

For SaltStack environments, add these states:

**Linux & macOS:**
```yaml
install_hostctl_unix:
  cmd.run:
    - name: curl -fsSL https://raw.githubusercontent.com/daddasoft/hostctl/main/install.sh | bash
    - creates: /usr/local/bin/hostctl
```

**Windows:**
```yaml
install_hostctl_windows:
  cmd.run:
    - name: irm https://raw.githubusercontent.com/daddasoft/hostctl/main/install.ps1 | iex
    - shell: powershell
    - creates: '%USERPROFILE%\.hostctl\bin\hostctl.exe'
```

### Manual download

Download the latest binary for your platform from the [Releases](https://github.com/daddasoft/hostctl/releases) page:

| Platform | File |
|---|---|
| Linux (x86_64) | `hostctl-linux-x86_64` |
| Linux (ARM64) | `hostctl-linux-aarch64` |
| macOS (Intel) | `hostctl-macos-x86_64` |
| macOS (Apple Silicon) | `hostctl-macos-aarch64` |
| Windows (x86_64) | `hostctl-windows-x86_64.exe` |
| Windows (ARM64) | `hostctl-windows-aarch64.exe` |

### Build from source

Requires [Rust](https://rustup.rs):

```bash
cargo build --release
# binary → target/release/hostctl  (or hostctl.exe on Windows)
```

### Uninstall

```powershell
irm https://raw.githubusercontent.com/daddasoft/hostctl/main/uninstall.ps1 | iex
```

```bash
curl -fsSL https://raw.githubusercontent.com/daddasoft/hostctl/main/uninstall.sh | sh
```

> **Note:** System hosts files normally require `sudo` or an Administrator
> terminal. If the existing file ACL already grants write access, hostctl can
> use its backed-up compatibility write path without directory elevation.

---

## Usage

```
hostctl [OPTIONS] <COMMAND>

Commands:
  add          Add one or more aliases for an IP address
  remove       Remove mappings by hostname or IP address
  get          Show mappings for one hostname
  search       Search IPs, hostnames, and comments
  update       Update or rename a hostname
  disable      Comment out a hostname mapping
  enable       Reactivate a disabled mapping
  status       Show active, disabled, duplicated, or missing state
  ensure       Idempotently ensure a mapping is present or absent
  list         List and filter entries
  export       Export hosts, JSON, YAML, or TOML
  import       Import entries from a file or stdin
  config       Manage .hostctl.toml
  group        Manage named groups
  profile      Manage environment profiles
  doctor       Diagnose the hosts file
  flush-dns    Flush the DNS cache
  resolve      Compare hosts-file and system resolution
  completion   Generate shell completions
  man          Generate a manual page
  self-update  Install the latest release

Options:
  --hosts <PATH>   Override the hosts file path (useful for testing)
  --config <PATH>  Override the project configuration path
  --dry-run        Preview exact proposed contents without changing the file
  --format <TYPE>  table, json, yaml, or plain
  --flush-dns      Flush DNS after a successful change
  -q, --quiet      Suppress successful output
  -v, --verbose    Print operational details
  -h, --help       Print help
  -V, --version    Print version
```

### Add an entry

```bash
# Basic
hostctl add 127.0.0.1 toto.local

# With an inline comment
hostctl add 192.168.1.10 myserver.local --comment "dev server"

# Multiple aliases
hostctl add 127.0.0.1 app.local api.local admin.local

# Force a second entry if hostname already exists
hostctl add 10.0.0.1 toto.local --force

# Replace the existing entry in-place
hostctl add 10.0.0.1 toto.local --overwrite   # or -o
```

> `--force` and `--overwrite` are mutually exclusive.

### Remove an entry

```bash
hostctl remove toto.local
hostctl remove --ip 192.168.1.10
hostctl remove toto.local --from-ip 10.0.0.1
```

If a line contains multiple aliases, only the requested hostname is removed.
The remaining aliases, inline comment, spacing, line endings, and final newline
are preserved.

### List all entries

```bash
hostctl list
hostctl list --hostname app.local --include-disabled
hostctl list --ipv4 --sort hostname
hostctl --format json list
```

```
IP ADDRESS           HOSTNAME(S)
--------------------------------------------------
127.0.0.1            localhost
192.168.1.10         myserver.local
```

### Test without touching the real hosts file

```bash
hostctl --hosts ./test-hosts add 127.0.0.1 toto.local
```

### Preview a change

```bash
hostctl --dry-run add 127.0.0.1 preview.local
hostctl --dry-run remove preview.local
```

Dry runs do not write the hosts file and do not create backups.

### Back up and restore

Every successful modification first creates a SHA-256-protected backup under
the user's local state directory. On Windows, backups are always stored at
`%LOCALAPPDATA%\hostctl\<hosts-file-id>\backups`.

```bash
# Create a manual backup
hostctl backup

# List backups and verify every checksum
hostctl backup list

# Restore the latest verified backup
hostctl restore

# Restore a specific ID from backup list
hostctl restore hosts.1785930000000000000.bak

# Preview a restore without writing
hostctl --dry-run restore latest
```

Writes are serialized with an inter-process lock and normally committed through
an atomic same-directory replacement. If the existing file is writable but its
protected parent directory is not, hostctl uses a user-local lock and performs
a flushed in-place update with an explicit warning. Backups remain in the same
user-local location in both modes. Symlinks and non-regular hosts-file targets
are rejected.

### Query and update

```bash
hostctl get app.local
hostctl search development
hostctl update app.local --ip 10.0.0.2 --rename app-v2.local
hostctl disable app-v2.local
hostctl status app-v2.local
hostctl enable app-v2.local
hostctl ensure present 10.0.0.2 app-v2.local
hostctl ensure absent old.local
```

### Import and export

```bash
hostctl export --file-format json --output entries.json
hostctl import entries.json --mode merge
hostctl --dry-run import entries.toml --mode replace
hostctl import entries.toml --mode replace --yes
```

Structured files use schema version `1` and an `entries` array. Replace mode
requires `--yes` unless it is a dry run.

### Groups and profiles

```bash
hostctl config init
hostctl group add dev 127.0.0.1 app.local api.local
hostctl group disable dev
hostctl group enable dev
hostctl profile create work dev
hostctl profile activate work
hostctl config validate
hostctl config diff
hostctl config apply
```

Managed entries are written only between `hostctl managed` markers. Manual and
system entries outside that block are never removed by group, profile, config,
or `clear` commands.

Example `.hostctl.toml`:

```toml
version = 1
active_profile = "work"

[groups.dev]
enabled = true

[[groups.dev.entries]]
ip = "127.0.0.1"
hostnames = ["app.local", "api.local"]
comment = "development"

[profiles.work]
groups = ["dev"]
```

### Diagnostics and integration

```bash
hostctl doctor
hostctl resolve app.local
hostctl flush-dns
hostctl completion powershell > hostctl.ps1
hostctl completion bash > hostctl.bash
hostctl man --output hostctl.1
hostctl self-update
hostctl self-update --check
hostctl self-uninstall
```

Stable error exit codes are `2` invalid input, `3` not found, `4` duplicate,
`5` permission denied, `6` lock contention, and `7` invalid data.

---

## Default hosts file paths

| OS | Path |
|---|---|
| Windows | `C:\Windows\System32\drivers\etc\hosts` |
| Linux / macOS | `/etc/hosts` |

The correct path is selected automatically at **compile time** via `#[cfg(windows)]`.

---

## Development Workflow

### Branching strategy

```
main          ← always stable, protected — release tags only
  └─ dev      ← integration branch, CI must pass before merge
       └─ feature/my-thing    ← daily work
       └─ fix/some-bug
```

| Branch | Direct push? | Requires |
|---|---|---|
| `main` | ❌ PR only | CI green + staging build passes |
| `dev` | ❌ PR only | CI green |
| `feature/*` / `fix/*` | ✅ | — |

### Day-to-day flow

```bash
# 1. Create a feature branch off dev
git checkout dev
git checkout -b feature/my-thing

# 2. Write code, then lint & test locally before pushing
cargo fmt
cargo clippy -- -D warnings
cargo test

# 3. Push and open a PR → dev
git push origin feature/my-thing
# CI runs automatically (fmt + clippy + tests on Linux, macOS, Windows)

# 4. Merge PR into dev

# 5. When ready to ship, open a PR dev → main
# A staging build runs and uploads real release binaries as PR artifacts
# Download and smoke-test them before merging

# 6. After merging, update Cargo.toml/Cargo.lock and push an annotated vX.Y.Z tag
# The tag triggers release.yml
```

---

## CI/CD Pipelines

### `ci.yml` — PR validation (every PR + push to `dev`)

Runs on **Linux, macOS, and Windows** in parallel:

- `cargo fmt --check`
- `cargo clippy -- -D warnings`
- `cargo test`
- `cargo build`

### `staging.yml` — Staging build (PR to `main`)

Builds release binaries for all platforms and uploads them as **PR artifacts** (kept for 5 days) so you can download and manually test the real binary before the release is cut.

### `release.yml` — Production release (on `v*.*.*` tag)

Triggered automatically by a `v*.*.*` tag. It:

1. Builds optimised release binaries for all 6 targets in parallel
2. Creates a GitHub Release with an auto-generated changelog
3. Creates amd64/arm64 Debian packages and package-manager manifests
4. Attaches binaries, package metadata, and `SHA256SUMS`

---

## Testing

Integration tests live in `tests/` and run the **real compiled binary** as a black-box:

```bash
cargo test
```

Tests use a temporary file instead of the real hosts file so no elevated privileges are needed and nothing on your system is modified.

The suite includes parser property tests and black-box coverage for entry
lifecycle, backups, structured import/export, groups, profiles, completions,
manual pages, and stable exit codes. Benchmark the 100,000-entry parser with:

```bash
cargo bench --bench large_hosts
```

The `fuzz/` package provides a `cargo fuzz run parse_hosts` target. Release
binaries and the published installers are smoke-tested by GitHub Actions.

---

## Versioning

Update `Cargo.toml` and `Cargo.lock`, validate, commit, and push an annotated tag:

```bash
cargo test
cargo clippy --all-targets -- -D warnings
git commit -am "chore: release v0.4.0"
git tag -a v0.4.0 -m "hostctl v0.4.0"
git push origin main v0.4.0
```

The tag triggers `release.yml`. External Homebrew, Scoop, WinGet, Chocolatey,
AUR, crates.io, and package-repository publication still requires the
maintainer credentials for those registries.

---

## License

MIT

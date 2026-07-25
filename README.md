# hostctl

A cross-platform CLI to manage your system **hosts file** — add, remove, list, preview, and undo changes safely.

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
| macOS (Intel) | `hostctl-macos-x86_64` |
| macOS (Apple Silicon) | `hostctl-macos-aarch64` |
| Windows (x86_64) | `hostctl-windows-x86_64.exe` |

### Build from source

Requires [Rust](https://rustup.rs):

```bash
cargo build --release
# binary → target/release/hostctl  (or hostctl.exe on Windows)
```

> **Note:** Writing to the system hosts file requires elevated privileges.  
> Run with `sudo` on Linux/macOS, or as **Administrator** on Windows.

---

## Usage

```
hostctl [OPTIONS] <COMMAND>

Commands:
  add     Add a new entry
  remove  Remove an entry by hostname
  list    List all active entries
  undo    Restore the most recent automatic backup
  help    Print help

Options:
  --hosts <PATH>   Override the hosts file path (useful for testing)
  --dry-run        Preview a change without writing any files
  -h, --help       Print help
  -V, --version    Print version
```

### Add an entry

```bash
# Basic
hostctl add 127.0.0.1 toto.local

# With an inline comment
hostctl add 192.168.1.10 myserver.local --comment "dev server"

# Force a second entry if hostname already exists
hostctl add 10.0.0.1 toto.local --force

# Replace the existing entry in-place
hostctl add 10.0.0.1 toto.local --overwrite   # or -o
```

> `--force` and `--overwrite` are mutually exclusive.

If a line contains several aliases, overwrite preserves the aliases that were
not selected. For example, overwriting `app.local` in this line does not remove
`localhost`:

```text
127.0.0.1 localhost app.local
```

### Remove an entry

```bash
hostctl remove toto.local
```

Removal is alias-aware as well: other hostnames and inline comments on the same
line are preserved.

### List all entries

```bash
hostctl list
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

Add `--dry-run` before or after the subcommand to see the affected lines without
changing the hosts file or creating a backup:

```bash
hostctl --dry-run add 127.0.0.1 app.local
hostctl remove old.local --dry-run
hostctl undo --dry-run
```

Preview output uses `-` for removed lines and `+` for added lines.

### Backups and undo

Every successful `add` or `remove` creates an automatic one-level backup next
to the hosts file:

| Hosts file | Backup |
|---|---|
| `/etc/hosts` | `/etc/hosts.hostctl.bak` |
| `C:\Windows\System32\drivers\etc\hosts` | `C:\Windows\System32\drivers\etc\hosts.hostctl.bak` |

Restore it with:

```bash
hostctl undo
```

The restore is also backed up, so running `hostctl undo` a second time switches
back to the version from before the first undo. Hosts-file and backup writes use
temporary files followed by an atomic replacement to avoid partially written
files.

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

# 6. After merging, cut a release
cargo release patch   # or minor / major
# This bumps Cargo.toml, commits, tags, and pushes → triggers release.yml
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

Triggered automatically by `cargo release`. It:

1. Builds optimised release binaries for all 4 targets in parallel
2. Creates a GitHub Release with an auto-generated changelog
3. Attaches all binaries as downloadable release assets

---

## Testing

Unit and file-level integration tests live in `src/tests.rs` and exercise the
parser, backup, undo, dry-run, and mutation behavior against temporary hosts
files:

```bash
cargo test
```

Tests use temporary directories instead of the real hosts file, so no elevated
privileges are needed and nothing on your system is modified.

---

## Versioning

This project uses [cargo-release](https://github.com/crate-ci/cargo-release):

```bash
cargo install cargo-release

cargo release patch   # 0.1.0 → 0.1.1
cargo release minor   # 0.1.1 → 0.2.0
cargo release major   # 0.2.0 → 1.0.0
```

Each command:
- Updates `version` in `Cargo.toml`
- Creates a commit `chore: release vX.Y.Z`
- Creates and pushes the git tag → triggers `release.yml`

---

## License

MIT

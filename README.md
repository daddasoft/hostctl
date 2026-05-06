# addhost

A cross-platform CLI to manage your system **hosts file** — add, remove, and list entries with safety guards against duplicates.

[![CI](https://github.com/daddasoft/addHost/actions/workflows/ci.yml/badge.svg)](https://github.com/daddasoft/addHost/actions/workflows/ci.yml)
[![Release](https://github.com/daddasoft/addHost/actions/workflows/release.yml/badge.svg)](https://github.com/daddasoft/addHost/releases)

---

## Installation

### Linux & macOS — one-liner

```bash
curl -fsSL https://raw.githubusercontent.com/daddasoft/addHost/main/install.sh | bash
```

### Windows — one-liner (PowerShell)

```powershell
irm https://raw.githubusercontent.com/daddasoft/addHost/main/install.ps1 | iex
```

Installs to `~\.addhost\bin` and adds it to your **user PATH** automatically.  
Override the install directory:

```powershell
$env:ADDHOST_INSTALL_DIR = "C:\Tools"
irm https://raw.githubusercontent.com/daddasoft/addHost/main/install.ps1 | iex
```

### Manual download

Download the latest binary for your platform from the [Releases](https://github.com/daddasoft/addHost/releases) page:

| Platform | File |
|---|---|
| Linux (x86_64) | `addhost-linux-x86_64` |
| macOS (Intel) | `addhost-macos-x86_64` |
| macOS (Apple Silicon) | `addhost-macos-aarch64` |
| Windows (x86_64) | `addhost-windows-x86_64.exe` |

### Build from source

Requires [Rust](https://rustup.rs):

```bash
cargo build --release
# binary → target/release/addhost  (or addhost.exe on Windows)
```

> **Note:** Writing to the system hosts file requires elevated privileges.  
> Run with `sudo` on Linux/macOS, or as **Administrator** on Windows.

---

## Usage

```
addhost [OPTIONS] <COMMAND>

Commands:
  add     Add a new entry
  remove  Remove an entry by hostname
  list    List all active entries
  help    Print help

Options:
  --hosts <PATH>   Override the hosts file path (useful for testing)
  -h, --help       Print help
  -V, --version    Print version
```

### Add an entry

```bash
# Basic
addhost add 127.0.0.1 toto.local

# With an inline comment
addhost add 192.168.1.10 myserver.local --comment "dev server"

# Force a second entry if hostname already exists
addhost add 10.0.0.1 toto.local --force

# Replace the existing entry in-place
addhost add 10.0.0.1 toto.local --overwrite   # or -o
```

> `--force` and `--overwrite` are mutually exclusive.

### Remove an entry

```bash
addhost remove toto.local
```

### List all entries

```bash
addhost list
```

```
IP ADDRESS           HOSTNAME(S)
--------------------------------------------------
127.0.0.1            localhost
192.168.1.10         myserver.local
```

### Test without touching the real hosts file

```bash
addhost --hosts ./test-hosts add 127.0.0.1 toto.local
```

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

Integration tests live in `tests/` and run the **real compiled binary** as a black-box:

```bash
cargo test
```

Tests use a temporary file instead of the real hosts file so no elevated privileges are needed and nothing on your system is modified.

Add [`tempfile`](https://crates.io/crates/tempfile) as a dev-dependency:

```toml
[dev-dependencies]
tempfile = "3"
```

Example test:

```rust
// tests/integration_test.rs
#[test]
fn duplicate_errors_without_flag() {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(tmp.path(), "127.0.0.1\ttoto.local\n").unwrap();

    let out = std::process::Command::new(env!("CARGO_BIN_EXE_addhost"))
        .args(["--hosts", tmp.path().to_str().unwrap(),
               "add", "127.0.0.1", "toto.local"])
        .output().unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("--force"));
}
```

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

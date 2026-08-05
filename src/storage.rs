use sha2::{Digest, Sha256};
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone)]
pub struct BackupInfo {
    pub id: String,
    pub path: PathBuf,
    pub checksum: String,
    pub valid: bool,
}

#[derive(Debug)]
pub struct MutationResult {
    pub before: Vec<u8>,
    pub after: Vec<u8>,
    pub backup: Option<BackupInfo>,
    pub atomic: bool,
}

pub fn read_utf8(path: &Path) -> io::Result<String> {
    validate_target(path)?;
    let bytes = fs::read(path).map_err(|error| path_error("read", path, error))?;
    String::from_utf8(bytes).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("'{}' is not valid UTF-8: {error}", path.display()),
        )
    })
}

pub fn mutate<F>(path: &Path, dry_run: bool, operation: F) -> io::Result<MutationResult>
where
    F: FnOnce(&str) -> io::Result<String>,
{
    let _lock = FileLock::acquire(path)?;
    validate_target(path)?;
    let before = fs::read(path).map_err(|error| path_error("read", path, error))?;
    let content = std::str::from_utf8(&before).map_err(|error| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("'{}' is not valid UTF-8: {error}", path.display()),
        )
    })?;
    let after = operation(content)?.into_bytes();

    if before == after || dry_run {
        return Ok(MutationResult {
            before,
            after,
            backup: None,
            atomic: true,
        });
    }

    let backup = create_backup_from_bytes(path, &before)?;
    let atomic = write_content(path, &after)?;
    Ok(MutationResult {
        before,
        after,
        backup: Some(backup),
        atomic,
    })
}

pub fn create_backup(path: &Path) -> io::Result<BackupInfo> {
    let _lock = FileLock::acquire(path)?;
    validate_target(path)?;
    let content = fs::read(path).map_err(|error| path_error("read", path, error))?;
    create_backup_from_bytes(path, &content)
}

pub fn list_backups(path: &Path) -> io::Result<Vec<BackupInfo>> {
    validate_target(path)?;
    let prefix = format!("{}.", target_name(path)?);
    let mut backups = Vec::new();

    let directory = backup_directory(path)?;
    if !directory.exists() {
        return Ok(backups);
    }
    validate_backup_directory(&directory)?;
    for item in fs::read_dir(&directory)
        .map_err(|error| path_error("read backup directory", &directory, error))?
    {
        let item = item.map_err(|error| path_error("read backup directory", &directory, error))?;
        let file_type = item
            .file_type()
            .map_err(|error| path_error("inspect backup", &item.path(), error))?;
        let id = item.file_name().to_string_lossy().into_owned();
        if !file_type.is_file() || !id.starts_with(&prefix) || !id.ends_with(".bak") {
            continue;
        }
        backups.push(inspect_backup(item.path(), id)?);
    }
    backups.sort_by(|left, right| right.id.cmp(&left.id));
    Ok(backups)
}

pub fn restore_backup(
    path: &Path,
    backup_id: Option<&str>,
    dry_run: bool,
) -> io::Result<MutationResult> {
    let _lock = FileLock::acquire(path)?;
    validate_target(path)?;
    let backup = select_backup(path, backup_id)?;
    if !backup.valid {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("backup '{}' failed checksum verification", backup.id),
        ));
    }

    let before = fs::read(path).map_err(|error| path_error("read", path, error))?;
    let after =
        fs::read(&backup.path).map_err(|error| path_error("read backup", &backup.path, error))?;
    if before == after || dry_run {
        return Ok(MutationResult {
            before,
            after,
            backup: None,
            atomic: true,
        });
    }

    let rollback = create_backup_from_bytes(path, &before)?;
    let atomic = write_content(path, &after)?;
    Ok(MutationResult {
        before,
        after,
        backup: Some(rollback),
        atomic,
    })
}

pub fn backup_directory(path: &Path) -> io::Result<PathBuf> {
    Ok(scoped_state_directory(path)?.join("backups"))
}

fn create_backup_from_bytes(path: &Path, content: &[u8]) -> io::Result<BackupInfo> {
    let checksum = sha256(content);
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock is before Unix epoch: {error}")))?
        .as_nanos();
    let id = format!("{}.{timestamp}.bak", target_name(path)?);
    let directory = backup_directory(path)?;
    create_backup_in(&directory, &id, &checksum, content)
}

fn create_backup_in(
    directory: &Path,
    id: &str,
    checksum: &str,
    content: &[u8],
) -> io::Result<BackupInfo> {
    ensure_backup_directory(directory)?;
    let backup_path = directory.join(id);
    let checksum_path = directory.join(format!("{id}.sha256"));

    if let Err(error) = write_new_file(&backup_path, content) {
        let _ = fs::remove_file(&backup_path);
        return Err(error);
    }
    let checksum_content = format!("{checksum}  {id}\n");
    if let Err(error) = write_new_file(&checksum_path, checksum_content.as_bytes()) {
        let _ = fs::remove_file(&backup_path);
        return Err(error);
    }
    Ok(BackupInfo {
        id: id.to_string(),
        path: backup_path,
        checksum: checksum.to_string(),
        valid: true,
    })
}

fn inspect_backup(path: PathBuf, id: String) -> io::Result<BackupInfo> {
    let content = fs::read(&path).map_err(|error| path_error("read backup", &path, error))?;
    let actual = sha256(&content);
    let checksum_path = path.with_file_name(format!("{id}.sha256"));
    let expected = fs::read_to_string(&checksum_path)
        .ok()
        .and_then(|value| value.split_whitespace().next().map(str::to_owned))
        .unwrap_or_default();
    Ok(BackupInfo {
        id,
        path,
        checksum: actual.clone(),
        valid: actual.eq_ignore_ascii_case(&expected),
    })
}

fn select_backup(path: &Path, backup_id: Option<&str>) -> io::Result<BackupInfo> {
    let backups = list_backups(path)?;
    if backups.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "no backups found for this hosts file",
        ));
    }
    match backup_id {
        None | Some("latest") => Ok(backups.into_iter().next().expect("backups is not empty")),
        Some(id) => {
            if Path::new(id).file_name() != Some(OsStr::new(id)) {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "backup ID must be a filename returned by 'hostctl backup list'",
                ));
            }
            backups
                .into_iter()
                .find(|backup| backup.id == id)
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotFound, format!("backup '{id}' not found"))
                })
        }
    }
}

fn ensure_backup_directory(path: &Path) -> io::Result<()> {
    if path.exists() {
        return validate_backup_directory(path);
    }
    fs::create_dir_all(path).map_err(|error| path_error("create backup directory", path, error))?;
    validate_backup_directory(path)
}

fn validate_backup_directory(path: &Path) -> io::Result<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| path_error("inspect", path, error))?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("backup path '{}' must be a real directory", path.display()),
        ));
    }
    Ok(())
}

fn validate_target(path: &Path) -> io::Result<()> {
    let metadata =
        fs::symlink_metadata(path).map_err(|error| path_error("inspect", path, error))?;
    if metadata.file_type().is_symlink() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("refusing to operate on symlink '{}'", path.display()),
        ));
    }
    if !metadata.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("'{}' is not a regular file", path.display()),
        ));
    }
    Ok(())
}

fn write_content(path: &Path, content: &[u8]) -> io::Result<bool> {
    match atomic_replace(path, content) {
        Ok(()) => Ok(true),
        Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
            write_in_place(path, content)?;
            Ok(false)
        }
        Err(error) => Err(error),
    }
}

fn atomic_replace(path: &Path, content: &[u8]) -> io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let permissions = fs::metadata(path)
        .map_err(|error| path_error("inspect", path, error))?
        .permissions();
    let temp_path = unique_temp_path(path)?;
    let result = (|| {
        let mut temp = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|error| path_error("create temporary file", &temp_path, error))?;
        temp.set_permissions(permissions)
            .map_err(|error| path_error("set temporary file permissions", &temp_path, error))?;
        temp.write_all(content)
            .map_err(|error| path_error("write temporary file", &temp_path, error))?;
        temp.sync_all()
            .map_err(|error| path_error("sync temporary file", &temp_path, error))?;
        drop(temp);
        replace_file(&temp_path, path)?;
        sync_directory(parent)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

fn write_in_place(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(false)
        .open(path)
        .map_err(|error| path_error("open hosts file for writing", path, error))?;
    file.write_all(content)
        .and_then(|_| file.set_len(content.len() as u64))
        .and_then(|_| file.sync_all())
        .map_err(|error| path_error("write hosts file", path, error))
}

fn unique_temp_path(path: &Path) -> io::Result<PathBuf> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let name = target_name(path)?;
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| io::Error::other(format!("system clock is before Unix epoch: {error}")))?
        .as_nanos();
    for attempt in 0..100 {
        let candidate = parent.join(format!(
            ".{name}.hostctl.tmp.{}.{timestamp}.{attempt}",
            std::process::id()
        ));
        if !candidate.exists() {
            return Ok(candidate);
        }
    }
    Err(io::Error::new(
        io::ErrorKind::AlreadyExists,
        "could not allocate a unique temporary filename",
    ))
}

#[cfg(not(windows))]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    fs::rename(source, destination)
        .map_err(|error| path_error("atomically replace", destination, error))
}

#[cfg(windows)]
fn replace_file(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    const MOVEFILE_REPLACE_EXISTING: u32 = 0x1;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x8;

    #[link(name = "Kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, replacement: *const u16, flags: u32) -> i32;
    }

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    let result = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if result == 0 {
        return Err(path_error(
            "atomically replace",
            destination,
            io::Error::last_os_error(),
        ));
    }
    Ok(())
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)
        .and_then(|directory| directory.sync_all())
        .map_err(|error| path_error("sync directory", path, error))
}

#[cfg(not(unix))]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

fn write_new_file(path: &Path, content: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|error| path_error("create", path, error))?;
    file.write_all(content)
        .and_then(|_| file.sync_all())
        .map_err(|error| path_error("write", path, error))
}

fn target_name(path: &Path) -> io::Result<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .filter(|name| !name.is_empty())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "hosts path has no filename"))
}

fn fallback_lock_path(path: &Path) -> io::Result<PathBuf> {
    Ok(scoped_state_directory(path)?.join("write.lock"))
}

fn scoped_state_directory(path: &Path) -> io::Result<PathBuf> {
    #[cfg(test)]
    let root = Some(
        path.parent()
            .unwrap_or_else(|| Path::new("."))
            .join(".hostctl-test-state"),
    );

    #[cfg(all(not(test), windows))]
    let root = std::env::var_os("HOSTCTL_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from));

    #[cfg(all(not(test), not(windows)))]
    let root = std::env::var_os("HOSTCTL_STATE_DIR")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("XDG_STATE_HOME").map(PathBuf::from))
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")));

    let root = root.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "cannot locate a writable user state directory",
        )
    })?;
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let mut identity = canonical.to_string_lossy().into_owned();
    if cfg!(windows) {
        identity.make_ascii_lowercase();
    }
    let scope = &sha256(identity.as_bytes())[..16];
    Ok(root.join("hostctl").join(scope))
}

fn sha256(content: &[u8]) -> String {
    let digest = Sha256::digest(content);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn path_error(action: &str, path: &Path, error: io::Error) -> io::Error {
    let elevation = if error.kind() == io::ErrorKind::PermissionDenied {
        if cfg!(windows) {
            " Run the terminal as Administrator."
        } else {
            " Run the command with sudo."
        }
    } else {
        ""
    };
    io::Error::new(
        error.kind(),
        format!("{action} '{}': {error}.{elevation}", path.display()),
    )
}

#[derive(Debug)]
struct FileLock {
    file: File,
}

impl FileLock {
    fn acquire(target: &Path) -> io::Result<Self> {
        let parent = target.parent().unwrap_or_else(|| Path::new("."));
        let lock_path = parent.join(format!("{}.hostctl.lock", target_name(target)?));
        match Self::open(&lock_path, target) {
            Ok(lock) => Ok(lock),
            Err(error) if error.kind() == io::ErrorKind::PermissionDenied => {
                let fallback = fallback_lock_path(target)?;
                let directory = fallback.parent().expect("fallback lock has a parent");
                fs::create_dir_all(directory)
                    .map_err(|error| path_error("create lock directory", directory, error))?;
                validate_backup_directory(directory)?;
                Self::open(&fallback, target)
            }
            Err(error) => Err(error),
        }
    }

    fn open(lock_path: &Path, target: &Path) -> io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|error| path_error("open lock file", lock_path, error))?;
        file.try_lock().map_err(|error| {
            io::Error::new(
                io::ErrorKind::WouldBlock,
                format!(
                    "another hostctl process is modifying '{}': {error}",
                    target.display()
                ),
            )
        })?;
        Ok(Self { file })
    }
}

impl Drop for FileLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn backup_checksum_detects_tampering() {
        let directory = tempdir().unwrap();
        let hosts = directory.path().join("hosts");
        fs::write(&hosts, b"127.0.0.1 localhost\n").unwrap();
        let backup = create_backup(&hosts).unwrap();
        fs::write(&backup.path, b"tampered").unwrap();
        let backups = list_backups(&hosts).unwrap();
        assert_eq!(backups.len(), 1);
        assert!(!backups[0].valid);
    }

    #[test]
    fn restore_rejects_tampered_backup() {
        let directory = tempdir().unwrap();
        let hosts = directory.path().join("hosts");
        fs::write(&hosts, b"original").unwrap();
        let backup = create_backup(&hosts).unwrap();
        fs::write(&backup.path, b"tampered").unwrap();
        let error = restore_backup(&hosts, Some(&backup.id), false).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
        assert_eq!(fs::read(&hosts).unwrap(), b"original");
    }

    #[test]
    fn mutation_creates_verified_backup() {
        let directory = tempdir().unwrap();
        let hosts = directory.path().join("hosts");
        fs::write(&hosts, b"before").unwrap();
        let result = mutate(&hosts, false, |_| Ok("after".to_string())).unwrap();
        assert_eq!(fs::read(&hosts).unwrap(), b"after");
        let backup = result.backup.unwrap();
        assert!(backup.valid);
        assert_eq!(
            backup.path.parent().unwrap(),
            backup_directory(&hosts).unwrap()
        );
    }

    #[test]
    fn dry_run_neither_writes_nor_creates_backup() {
        let directory = tempdir().unwrap();
        let hosts = directory.path().join("hosts");
        fs::write(&hosts, b"before").unwrap();
        let result = mutate(&hosts, true, |_| Ok("after".to_string())).unwrap();
        assert_eq!(fs::read(&hosts).unwrap(), b"before");
        assert!(result.backup.is_none());
        assert!(!backup_directory(&hosts).unwrap().exists());
    }

    #[test]
    fn symlink_targets_are_rejected() {
        let directory = tempdir().unwrap();
        let hosts = directory.path().join("hosts");
        fs::write(&hosts, b"content").unwrap();
        let link = directory.path().join("hosts-link");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&hosts, &link).unwrap();
        #[cfg(windows)]
        if std::os::windows::fs::symlink_file(&hosts, &link).is_err() {
            return;
        }
        assert_eq!(
            read_utf8(&link).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }

    #[test]
    fn a_second_writer_cannot_acquire_the_lock() {
        let directory = tempdir().unwrap();
        let hosts = directory.path().join("hosts");
        fs::write(&hosts, b"content").unwrap();
        let _first = FileLock::acquire(&hosts).unwrap();
        let error = FileLock::acquire(&hosts).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::WouldBlock);
    }

    #[test]
    fn directories_are_rejected_as_hosts_targets() {
        let directory = tempdir().unwrap();
        let error = read_utf8(directory.path()).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn permission_errors_include_elevation_guidance() {
        let path = Path::new("hosts");
        let error = path_error(
            "write",
            path,
            io::Error::new(io::ErrorKind::PermissionDenied, "denied"),
        );
        let expected = if cfg!(windows) {
            "Administrator"
        } else {
            "sudo"
        };
        assert!(error.to_string().contains(expected));
    }

    #[test]
    fn compatibility_write_truncates_old_trailing_bytes() {
        let directory = tempdir().unwrap();
        let hosts = directory.path().join("hosts");
        fs::write(&hosts, b"a much longer original value").unwrap();
        write_in_place(&hosts, b"short").unwrap();
        assert_eq!(fs::read(&hosts).unwrap(), b"short");
    }
}

//! Durable local file replacement shared by documents and configuration.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Replace a file atomically after its complete contents have reached the OS.
///
/// A same-directory temporary file is committed with the platform's native
/// replace primitive, including replacement of existing files on Windows.
pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;

    let existing_permissions = fs::metadata(path)
        .ok()
        .map(|metadata| metadata.permissions());

    let mut file = atomic_write_file::AtomicWriteFile::open(path)?;
    if let Some(permissions) = existing_permissions {
        file.set_permissions(permissions)?;
    }
    file.write_all(contents)?;
    file.flush()?;
    file.sync_all()?;
    file.commit()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temporary_directory(test_name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "numr-tui-{test_name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[test]
    fn replaces_complete_file_and_cleans_temporary_file() {
        let directory = temporary_directory("atomic-write");
        let path = directory.join("document.numr");
        fs::create_dir_all(&directory).unwrap();
        fs::write(&path, "old contents").unwrap();

        atomic_write(&path, b"new contents").unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "new contents");
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn does_not_replace_destination_when_rename_fails() {
        let directory = temporary_directory("atomic-write-failure");
        let destination = directory.join("destination");
        fs::create_dir_all(&destination).unwrap();
        fs::write(destination.join("sentinel"), "untouched").unwrap();

        assert!(atomic_write(&destination, b"replacement").is_err());

        assert_eq!(
            fs::read_to_string(destination.join("sentinel")).unwrap(),
            "untouched"
        );
        assert!(fs::read_dir(&directory).unwrap().count() >= 1);
        fs::remove_dir_all(directory).unwrap();
    }
}

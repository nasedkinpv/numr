//! Durable native persistence shared by documents, configuration, and rate caches.

use std::fs;
use std::io::{self, Write};
use std::path::Path;

/// Replace a file atomically using a same-directory temporary file and the
/// platform's native replacement primitive (including on Windows).
///
/// On Unix, creation is private (0600 files, 0700 new parent directories), while
/// modification preserves the existing mode for intentionally shared `.numr`
/// files. Existing directories are never chmodded.
pub fn atomic_write(path: &Path, contents: &[u8]) -> io::Result<()> {
    atomic_write_staged(path, contents).map_err(|error| match error {
        WriteError::CreateDirectory(error) | WriteError::Write(error) => error,
    })
}

/// Retain the cache's distinction between parent creation and file-write errors.
#[derive(Debug)]
pub(crate) enum WriteError {
    CreateDirectory(io::Error),
    Write(io::Error),
}

pub(crate) fn atomic_write_staged(path: &Path, contents: &[u8]) -> Result<(), WriteError> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder
        .create(parent)
        .map_err(WriteError::CreateDirectory)?;
    write_file(path, contents).map_err(WriteError::Write)
}

fn write_file(path: &Path, contents: &[u8]) -> io::Result<()> {
    let existing_permissions = match fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            // Reject directories before opening a temporary file. In particular,
            // the Linux O_TMPFILE backend can leave a link after a failed rename.
            return Err(io::Error::new(
                io::ErrorKind::IsADirectory,
                "cannot replace a directory with a file",
            ));
        }
        Ok(metadata) => Some(metadata.permissions()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    };

    #[cfg(unix)]
    let options = {
        use atomic_write_file::unix::OpenOptionsExt as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        // Keep the temporary file private even before writing any content.
        let mut options = atomic_write_file::OpenOptions::new();
        options.mode(0o600).preserve_mode(false);
        options
    };
    #[cfg(not(unix))]
    let options = atomic_write_file::OpenOptions::new();
    let mut file = options.open(path)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Set explicitly on the temporary file; do not depend on the user's umask.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    file.write_all(contents)?;
    file.flush()?;
    if let Some(permissions) = existing_permissions {
        // Writing can clear set-ID bits, so restore the exact mode after writing.
        file.set_permissions(permissions)?;
    }
    file.sync_all()?;
    file.commit()
}

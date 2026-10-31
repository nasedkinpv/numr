//! Durable local file replacement shared by documents and configuration.

pub(crate) use numr_core::persistence::atomic_write;

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    #[cfg(unix)]
    use std::io::Write;
    #[cfg(unix)]
    use std::path::Path;
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

    #[cfg(unix)]
    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[cfg(unix)]
    #[test]
    fn private_new_document_and_nested_directories() {
        let directory = temporary_directory("private-new");
        let parent = directory.join("nested");
        let path = parent.join("document.numr");
        atomic_write(&path, b"private").unwrap();
        let modes = (mode(&path), mode(&directory), mode(&parent));
        fs::remove_dir_all(directory).unwrap();
        assert_eq!(modes, (0o600, 0o700, 0o700));
    }

    #[cfg(unix)]
    #[test]
    fn private_overwrite_preserves_file_and_existing_directory_modes() {
        use std::os::unix::fs::PermissionsExt;
        let directory = temporary_directory("private-overwrite");
        fs::create_dir(&directory).unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        let path = directory.join("document.numr");
        for permissions in [0o644, 0o600, 0o640, 0o6750] {
            fs::write(&path, b"old").unwrap();
            fs::set_permissions(&path, fs::Permissions::from_mode(permissions)).unwrap();
            atomic_write(&path, b"new").unwrap();
            assert_eq!(mode(&path), permissions);
            assert_eq!(mode(&directory), 0o755);
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn private_external_append_then_save_replaces_content() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let directory = temporary_directory("private-append");
        fs::create_dir(&directory).unwrap();
        let path = directory.join("document.numr");
        let mut external = fs::OpenOptions::new()
            .create_new(true)
            .append(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        external.write_all(b"external append").unwrap();
        drop(external);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        atomic_write(&path, b"saved content").unwrap();
        assert_eq!(fs::read(&path).unwrap(), b"saved content");
        assert_eq!(mode(&path), 0o600);
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn private_bare_relative_filename() {
        const CHILD: &str = "NUMR_PERSISTENCE_RELATIVE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            atomic_write(PathBuf::from("document.numr").as_path(), b"relative").unwrap();
            assert_eq!(fs::read("document.numr").unwrap(), b"relative");
            return;
        }
        let directory = temporary_directory("relative");
        fs::create_dir(&directory).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "persistence::tests::private_bare_relative_filename",
            ])
            .env(CHILD, "1")
            .current_dir(&directory)
            .status()
            .unwrap();
        fs::remove_dir_all(directory).unwrap();
        assert!(status.success());
    }

    // Exercise the same writer used by Config::save without changing the
    // process-global configuration environment in parallel tests.
    #[cfg(unix)]
    #[test]
    fn private_config_uses_same_policy() {
        use std::os::unix::fs::PermissionsExt;
        let directory = temporary_directory("private-config");
        let path = directory.join("numr/config.toml");
        let content = toml::to_string_pretty(&crate::config::Config::default()).unwrap();
        atomic_write(&path, content.as_bytes()).unwrap();
        assert_eq!(mode(&path), 0o600);
        assert_eq!(mode(&directory), 0o700);
        assert_eq!(mode(path.parent().unwrap()), 0o700);
        for permissions in [0o644, 0o600] {
            fs::set_permissions(&path, fs::Permissions::from_mode(permissions)).unwrap();
            atomic_write(&path, content.as_bytes()).unwrap();
            assert_eq!(mode(&path), permissions);
        }
        fs::remove_dir_all(directory).unwrap();
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

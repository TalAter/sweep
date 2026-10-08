//! Files that readers never see half-written: the bytes go to a private temp file in
//! the target's directory, are synced, and only then get a name.
use std::{
    io::{self, Write},
    path::{Path, PathBuf},
};

/// A fully written and synced file that does not have its final name yet.
/// Dropping it unpublished removes the temp file.
pub(crate) struct Prepared {
    tmp: tempfile::NamedTempFile,
    path: PathBuf,
}

/// Writes `bytes` next to `path` (creating the directory) with `mode`. The temp file
/// starts private (0600) and is widened only after the write, so secrets are never
/// readable by others, even briefly.
pub(crate) fn prepare(path: &Path, bytes: &[u8], mode: u32) -> io::Result<Prepared> {
    let dir = path.parent().filter(|p| !p.as_os_str().is_empty());
    let dir = dir.ok_or_else(|| io::Error::other("path has no parent directory"))?;
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("path has no file name"))?
        .to_string_lossy();
    std::fs::create_dir_all(dir)?;
    let mut tmp = tempfile::Builder::new()
        .prefix(&format!(".{name}."))
        .suffix(".tmp")
        .tempfile_in(dir)?;
    tmp.write_all(bytes)?;
    tmp.as_file().sync_all()?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        tmp.as_file()
            .set_permissions(std::fs::Permissions::from_mode(mode))?;
    }
    #[cfg(not(unix))]
    let _ = mode;
    Ok(Prepared {
        tmp,
        path: path.to_path_buf(),
    })
}

impl Prepared {
    /// Renames over whatever is at the path.
    pub(crate) fn replace(self) -> io::Result<()> {
        self.tmp.persist(&self.path).map_err(|e| e.error)?;
        Ok(())
    }
    /// Publishes without replacing an existing file; `AlreadyExists` if one is there.
    pub(crate) fn create_new(self) -> io::Result<()> {
        self.tmp
            .persist_noclobber(&self.path)
            .map_err(|e| e.error)?;
        Ok(())
    }
}

/// `prepare` then `replace` in one step.
pub(crate) fn write_atomically(path: &Path, bytes: &[u8], mode: u32) -> io::Result<()> {
    prepare(path, bytes, mode)?.replace()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn replace_overwrites_and_create_new_refuses_an_existing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested/file.txt");
        write_atomically(&path, b"one", 0o644).unwrap();
        write_atomically(&path, b"two", 0o644).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"two");
        let error = prepare(&path, b"three", 0o644)
            .unwrap()
            .create_new()
            .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(std::fs::read(&path).unwrap(), b"two", "untouched");
        assert_eq!(
            names(&dir.path().join("nested")),
            ["file.txt"],
            "no temp left"
        );
        std::fs::remove_file(&path).unwrap();
        prepare(&path, b"three", 0o600)
            .unwrap()
            .create_new()
            .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"three");
    }

    #[test]
    fn dropping_a_prepared_file_publishes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file.txt");
        drop(prepare(&path, b"x", 0o644).unwrap());
        assert!(names(dir.path()).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn mode_is_applied_before_publication() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("secret");
        let prepared = prepare(&path, b"x", 0o600).unwrap();
        let tmp_mode = prepared
            .tmp
            .as_file()
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(tmp_mode, 0o600);
        prepared.create_new().unwrap();
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
}

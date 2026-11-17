use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn open(locator: &str, html: &str) -> Result<PathBuf, Error> {
    let path = write_at(&std::env::temp_dir(), locator, html)?;
    launch(&path)?;
    Ok(path)
}

fn write_at(dir: &Path, locator: &str, html: &str) -> Result<PathBuf, Error> {
    let path = dir.join(format!("{locator}.html"));
    let mut file = create(&path).map_err(write_error)?;
    #[cfg(unix)]
    prepare(&file).map_err(write_error)?;
    file.write_all(html.as_bytes()).map_err(write_error)?;
    Ok(path)
}

fn write_error(error: std::io::Error) -> Error {
    Error::Write(error.to_string())
}

fn create(path: &Path) -> std::io::Result<std::fs::File> {
    let mut options = OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(not(unix))]
    {
        options.truncate(true);
    }
    options.open(path).map_err(refuse_special)
}

fn refuse_special(error: std::io::Error) -> std::io::Error {
    #[cfg(unix)]
    if matches!(error.raw_os_error(), Some(libc::ELOOP | libc::ENXIO)) {
        return refused();
    }
    error
}

#[cfg(unix)]
fn prepare(file: &std::fs::File) -> std::io::Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    let meta = file.metadata()?;
    if !meta.is_file() || meta.uid() != unsafe { libc::geteuid() } as u32 {
        return Err(refused());
    }
    file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    file.set_len(0)?;
    Ok(())
}

#[cfg(unix)]
fn refused() -> std::io::Error {
    std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "not a regular file owned by this user",
    )
}

fn launch(path: &Path) -> Result<(), Error> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else {
        "xdg-open"
    };
    let output = Command::new(program)
        .arg(path)
        .output()
        .map_err(|error| Error::Launch(error.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stderr = stderr.trim();
    let message = if stderr.is_empty() {
        format!("{program} exited with {}", output.status)
    } else {
        stderr.to_string()
    };
    Err(Error::Launch(message))
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum Error {
    Write(String),
    Launch(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Write(message) => write!(f, "Could not write the note: {message}"),
            Error::Launch(message) => f.write_str(message),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_keeps_the_document_bytes() {
        let dir = tempfile::tempdir().expect("temp dir");
        let html = "<!doctype html><title>Opened note</title><p>violet ink</p>";
        let path = write_at(dir.path(), "0123456789abcdef0123", html).expect("write");
        assert_eq!(
            path.file_name().and_then(|name| name.to_str()),
            Some("0123456789abcdef0123.html")
        );
        assert_eq!(std::fs::read_to_string(&path).expect("read"), html);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path)
                .expect("metadata")
                .permissions()
                .mode();
            assert_eq!(mode & 0o777, 0o600);
        }
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_keeps_its_target() {
        let dir = tempfile::tempdir().expect("temp dir");
        let target = dir.path().join("target");
        std::fs::write(&target, "keep").expect("target");
        std::os::unix::fs::symlink(&target, dir.path().join("0123456789abcdef0123.html"))
            .expect("symlink");
        let html = "<p>violet ink</p>";
        let error = write_at(dir.path(), "0123456789abcdef0123", html).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Could not write the note: not a regular file owned by this user"
        );
        assert_eq!(std::fs::read_to_string(&target).expect("read"), "keep");
    }

    #[cfg(unix)]
    #[test]
    fn a_fifo_is_refused() {
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("0123456789abcdef0123.html");
        let bytes = std::os::unix::ffi::OsStrExt::as_bytes(path.as_os_str());
        let c_path = std::ffi::CString::new(bytes).expect("path");
        assert_eq!(unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) }, 0);
        let html = "<p>violet ink</p>";
        let error = write_at(dir.path(), "0123456789abcdef0123", html).unwrap_err();
        assert_eq!(
            error.to_string(),
            "Could not write the note: not a regular file owned by this user"
        );
    }

    #[cfg(unix)]
    #[test]
    fn an_existing_note_file_is_replaced() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("temp dir");
        let path = dir.path().join("0123456789abcdef0123.html");
        std::fs::write(&path, "previous page, longer than the next one").expect("previous");
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).expect("mode");
        let html = "<p>violet ink</p>";
        let written = write_at(dir.path(), "0123456789abcdef0123", html).expect("write");
        assert_eq!(std::fs::read_to_string(&written).expect("read"), html);
        let mode = std::fs::metadata(&written)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}

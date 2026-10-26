use std::{
    fs,
    fs::remove_file,
    io,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::Path,
};

use crate::internal_events::UnixSocketFileDeleteError;

pub const UNNAMED_SOCKET_HOST: &str = "(unnamed)";

///
/// # Errors
///
/// Returns an error if the socket permissions cannot be changed.
pub fn change_socket_permissions(path: &Path, perms: Option<u32>) -> crate::Result<()> {
    if let Some(mode) = perms {
        match fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
            Ok(()) => debug!(message = "Socket permissions updated.", permission = mode),
            Err(e) => {
                if let Err(error) = remove_file(path) {
                    emit!(UnixSocketFileDeleteError { path, error });
                }
                return Err(Box::new(e));
            }
        }
    }
    Ok(())
}

/// Removes an orphaned unix socket file when a previous process did not shut down cleanly
/// so that a new socket can be cleanly bound there.
///
/// The file is only removed if it is a socket and `connect` reports that nothing is listening on
/// it, to avoid clobbering a socket owned by another process.
pub fn remove_stale_socket(path: &Path, connect: impl FnOnce(&Path) -> io::Result<()>) {
    let is_socket = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket());
    if !is_socket {
        return;
    }

    if let Err(error) = connect(path)
        && error.kind() == io::ErrorKind::ConnectionRefused
    {
        info!(message = "Removing stale socket file.", ?path);
        if let Err(error) = remove_file(path) {
            emit!(UnixSocketFileDeleteError { path, error });
        }
    }
}

#[cfg(test)]
mod tests {
    use std::os::unix::net::{UnixDatagram, UnixListener, UnixStream};

    use super::*;

    fn connect_datagram(path: &Path) -> io::Result<()> {
        UnixDatagram::unbound()?.connect(path)
    }

    fn connect_stream(path: &Path) -> io::Result<()> {
        UnixStream::connect(path).map(drop)
    }

    #[test]
    fn removes_stale_datagram_socket() {
        let path = tempfile::tempdir().unwrap().keep().join("stale");
        drop(UnixDatagram::bind(&path).unwrap());
        assert!(path.exists());

        remove_stale_socket(&path, connect_datagram);
        assert!(!path.exists());
        UnixDatagram::bind(&path).unwrap();
    }

    #[test]
    fn removes_stale_stream_socket() {
        let path = tempfile::tempdir().unwrap().keep().join("stale");
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());

        remove_stale_socket(&path, connect_stream);
        assert!(!path.exists());
        UnixListener::bind(&path).unwrap();
    }

    #[test]
    fn keeps_live_socket() {
        let path = tempfile::tempdir().unwrap().keep().join("live");
        let _socket = UnixDatagram::bind(&path).unwrap();

        remove_stale_socket(&path, connect_datagram);
        assert!(path.exists());
    }

    #[test]
    fn keeps_regular_file() {
        let path = tempfile::tempdir().unwrap().keep().join("file");
        fs::write(&path, "data").unwrap();

        remove_stale_socket(&path, connect_datagram);
        assert!(path.exists());
    }
}

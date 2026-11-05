use std::{
    fs,
    fs::remove_file,
    io,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::Path,
    time::Duration,
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

const STALE_SOCKET_PROBE_TIMEOUT: Duration = Duration::from_secs(1);

/// Removes an orphaned unix socket file when a previous process did not shut down cleanly
/// so that a new socket can be cleanly bound there.
///
/// The file is only removed if it is a socket and `connect` reports that nothing is listening on
/// it, to avoid clobbering a socket owned by another process.
///  If the connect probe does not complete within [`STALE_SOCKET_PROBE_TIMEOUT`] the socket
///  is assumed to be live and is left in place.
pub async fn remove_stale_socket(path: &Path, connect: impl AsyncFnOnce(&Path) -> io::Result<()>) {
    let is_socket = fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_socket());
    if !is_socket {
        return;
    }

    if let Ok(Err(error)) = tokio::time::timeout(STALE_SOCKET_PROBE_TIMEOUT, connect(path)).await
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
    use std::os::unix::net::{UnixDatagram, UnixListener};

    use super::*;

    async fn connect_datagram(path: &Path) -> io::Result<()> {
        tokio::net::UnixDatagram::unbound()?.connect(path)
    }

    async fn connect_stream(path: &Path) -> io::Result<()> {
        tokio::net::UnixStream::connect(path).await.map(drop)
    }

    #[tokio::test]
    async fn removes_stale_datagram_socket() {
        let path = tempfile::tempdir().unwrap().keep().join("stale");
        drop(UnixDatagram::bind(&path).unwrap());
        assert!(path.exists());

        remove_stale_socket(&path, connect_datagram).await;
        assert!(!path.exists());
        UnixDatagram::bind(&path).unwrap();
    }

    #[tokio::test]
    async fn removes_stale_stream_socket() {
        let path = tempfile::tempdir().unwrap().keep().join("stale");
        drop(UnixListener::bind(&path).unwrap());
        assert!(path.exists());

        remove_stale_socket(&path, connect_stream).await;
        assert!(!path.exists());
        UnixListener::bind(&path).unwrap();
    }

    #[tokio::test]
    async fn keeps_live_socket() {
        let path = tempfile::tempdir().unwrap().keep().join("live");
        let _socket = UnixDatagram::bind(&path).unwrap();

        remove_stale_socket(&path, connect_datagram).await;
        assert!(path.exists());
    }

    #[tokio::test]
    async fn keeps_regular_file() {
        let path = tempfile::tempdir().unwrap().keep().join("file");
        fs::write(&path, "data").unwrap();

        remove_stale_socket(&path, connect_datagram).await;
        assert!(path.exists());
    }

    #[tokio::test]
    async fn keeps_socket_when_probe_times_out() {
        let path = tempfile::tempdir().unwrap().keep().join("hung");
        let _listener = UnixListener::bind(&path).unwrap();

        tokio::time::pause();
        remove_stale_socket(&path, async |_: &Path| std::future::pending().await).await;
        assert!(path.exists());
    }
}

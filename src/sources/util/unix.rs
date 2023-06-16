use std::os::unix::fs::PermissionsExt;
use std::{fs, fs::remove_file, path::Path};

use crate::internal_events::UnixSocketFileDeleteError;

pub const UNNAMED_SOCKET_HOST: &str = "(unnamed)";

pub fn change_socket_permissions(path: &Path, perms: Option<u32>) -> crate::Result<()> {
    if let Some(mode) = perms {
        match fs::set_permissions(path, fs::Permissions::from_mode(mode)) {
            Ok(_) => debug!(message = "Socket permissions updated.", permission = mode),
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

/// This is a structure which represents what kind of metadata should be
/// _collected_ by unix_stream.rs & unix_datagram.rs. I would do this with
/// some kind of flags structure, but Rust doesn't have one, so I guess a
/// struct-of-bools works.
#[derive(Default, Copy, Clone, Debug)]
pub struct UnixSocketMetadataCollectTypes {
    /// Use getpeername(2) (on stream sockets) or read the struct sockaddr
    /// argument from recvfrom(2) (on datagram sockets) to get the bound name
    /// of the other half of the socket.
    pub peer_path: bool,
}

/// This structure defines the various kinds of metadata we can
/// collect off a connected unix-domain socket and expose as source fields.
pub struct UnixSocketMetadata {
    /// The peer address of the socket, as returned from getpeername(2). This
    /// will usually not be set (unless the connecting peer has explicitly
    /// bound their socket to a path).
    pub peer_path: Option<String>,
}

impl UnixSocketMetadata {
    pub fn peer_path_or_default(&self) -> &str {
        match &self.peer_path {
            Some(path) => &path,
            None => UNNAMED_SOCKET_HOST,
        }
    }
}

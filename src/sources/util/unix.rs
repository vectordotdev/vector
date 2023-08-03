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

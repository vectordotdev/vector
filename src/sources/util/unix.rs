use std::{fs, fs::remove_file, os::unix::fs::PermissionsExt, path::Path};

use crate::internal_events::UnixSocketFileDeleteError;

pub const UNNAMED_SOCKET_HOST: &str = "(unnamed)";

// https://github.com/vectordotdev/vector/issues/23659
#[allow(
    clippy::missing_errors_doc,
    reason = "Audit and document the existing error contracts separately from lint enforcement."
)]
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

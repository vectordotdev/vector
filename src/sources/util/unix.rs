use std::{
    fs,
    fs::remove_file,
    os::unix::{ffi::OsStrExt, fs::PermissionsExt},
    path::Path,
};

use crate::internal_events::UnixSocketFileDeleteError;

pub const UNNAMED_SOCKET_HOST: &str = "(unnamed)";

pub fn change_socket_ownership(
    path: &Path,
    uid: Option<u32>,
    gid: Option<u32>,
) -> crate::Result<()> {
    if uid.is_none() && gid.is_none() {
        return Ok(());
    }

    let uid = uid.map_or(!0, libc::uid_t::from);
    let gid = gid.map_or(!0, libc::gid_t::from);
    // SAFETY: `path` is converted to a nul-terminated C string by `CString`; `chown` does not
    // retain the pointer beyond this call.
    let path_cstr = std::ffi::CString::new(path.as_os_str().as_bytes())?;
    let result = unsafe { libc::chown(path_cstr.as_ptr(), uid, gid) };

    if result == 0 {
        debug!(message = "Socket ownership updated.", uid, gid);
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    if let Err(error) = remove_file(path) {
        emit!(UnixSocketFileDeleteError { path, error });
    }
    Err(Box::new(error))
}

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

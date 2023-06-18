use std::{
    collections::BTreeMap,
    fs,
    fs::remove_file,
    mem::ManuallyDrop,
    os::{
        unix::fs::{PermissionsExt,MetadataExt},
        fd::{AsRawFd, FromRawFd, RawFd, IntoRawFd},
    },
    panic::resume_unwind,
    path::Path,
};
use tokio::{
    net::unix::{uid_t, gid_t, pid_t, UCred},
    task::spawn_blocking,
};
use vrl::value::Value;
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
/// struct-of-booleans works.
#[derive(Default, Copy, Clone, Debug)]
pub struct UnixSocketMetadataCollectTypes {
    /// Use getpeername(2) (on stream sockets) or read the struct sockaddr
    /// argument from recvfrom(2) (on datagram sockets) to get the bound name
    /// of the other half of the socket.
    pub peer_path: bool,

    /// Uses fstat(2) to get the inode/device of the connected socket (in
    /// stream socket mode). Ignored in datagram socket mode (it would simply
    /// return the same inode/dev every time, for the listener socket)
    pub socket_inode: bool,

    /// Use getsockopt(2) SO_PEERCRED or LOCAL_PEERCRED etc to query the creds
    /// of the process that opened the socket
    pub peer_creds: bool,
}

#[derive(Copy, Clone, Debug)]
pub struct SocketInode {
    pub dev: u64,
    pub ino: u64,
}

impl Into<Value> for SocketInode {
    fn into(self) -> Value {
        let mut map = BTreeMap::new();
        // really want "as i64" here - actually despite claiming to return
        // a u64, the value we get from fstat(2) for dev on some platforms
        // is the bit pattern for -1. So, reinterpret the "u64" as signed.
        map.insert("dev".to_string(), Value::Integer(self.dev as i64));
        map.insert("ino".to_string(), Value::Integer(self.ino as i64));
        Value::Object(map)
    }
}

// This is more or less a copy of "struct UCred", but we need to
// implement Into<Value> for it, and we can't do that to struct UCred itself.
// in this crate.
#[derive(Copy, Clone, Debug)]
pub struct SocketCredentials {
    // These uid_t/gid_t/pid_t types are from tokio::net::unix. Really, we should
    // use the libc ones, but the libc crate is not actually declared as a dependency
    // in Cargo.toml on all platforms.
    pub uid: uid_t,
    pub gid: gid_t,
    pub pid: Option<pid_t>
}

impl Into<Value> for SocketCredentials {
    fn into(self) -> Value {
        let mut map = BTreeMap::new();
        map.insert("uid".to_string(), Value::Integer(self.uid.into()));
        map.insert("gid".to_string(), Value::Integer(self.gid.into()));
        let pid_val = match self.pid {
            Some(pid) => Value::Integer(pid.into()),
            None => Value::Null,
        };
        map.insert("pid".to_string(), pid_val);
        Value::Object(map)
    }
}

impl From<UCred> for SocketCredentials {
    fn from(other: UCred) -> Self {
        Self {
            uid: other.uid(),
            gid: other.gid(),
            pid: other.pid().clone(),
        }
    }
}

/// This structure defines the various kinds of metadata we can
/// collect off a connected unix-domain socket and expose as source fields.
#[derive(Clone, Debug)]
pub struct UnixSocketMetadata {
    /// The peer address of the socket, as returned from getpeername(2). This
    /// will usually not be set (unless the connecting peer has explicitly
    /// bound their socket to a path).
    pub peer_path: Option<String>,

    /// The inode/device of the socket, as collected from fstat(2). This only
    /// makes sense for stream sockets, not datagram ones.
    pub socket_inode: Option<SocketInode>,

    /// The peer credentials of the process which connected to the socket,
    /// reported by getsockopt(2) SO_PEERCRED/LOCAL_PEERCRED/etc. This may not
    /// be the same as the process currently writing to the socket, because
    /// the socket could have been passed down to a child process or sent
    /// to a different process.
    pub peer_creds: Option<SocketCredentials>,
}

impl UnixSocketMetadata {
    pub fn peer_path_or_default(&self) -> &str {
        match &self.peer_path {
            Some(path) => &path,
            None => UNNAMED_SOCKET_HOST,
        }
    }
}


/// Collects the device & inode number for a socket.
pub async fn get_socket_inode<T : AsRawFd>(socket: &T) -> Result<SocketInode, Box<dyn std::error::Error>> {
    // Get the socket file descriptor. This is the actual integer in use by the socket,
    // not a dup(2) of it.
    let socket_fd = socket.as_raw_fd();

    // This needs to be done in a task, because fstat(2) is (technically) blocking.
    // Tokio's file::metadata() essentially does the same thing.
    spawn_blocking(move || {
        // Construct a new std::file from it.
        // We _really_ don't want std_file to run its drop (which would close the actual file!)
        // under _any_ circumstances, because that's going to actually close the actual socket.
        //
        // Safety: from_raw_fd isn't really memory-unsafe, it's just warning us that
        // double-closing of the descriptor might happen if it's still owned elsewhere.
        // NonClosingFile fixes that.
        let non_closing_socket_file = unsafe { NonClosingFile::from_raw_fd(socket_fd) };
        non_closing_socket_file.file.metadata()
    }).await
        .map_err(|error| -> Box<dyn std::error::Error> {
            // Propagate panics from the task
            match error.try_into_panic() {
                Ok(panic_reason) => resume_unwind(panic_reason),
                // Err(error) => error.into(),
                Err(error) => error.into(),
            }
        })
        .and_then(|metadata_result| {
            metadata_result.map_err(|error| -> Box<dyn std::error::Error> { error.into() })
        })
        .map(|metadata| {
            // Construct an inode object from the metadata.
            SocketInode {
                dev: metadata.dev(),
                ino: metadata.ino(),
            }
        })
}

// NonClosingFile implementation which can be constructed with a RawFd,
// and does not drop & close the underlying file when it is dropped.
struct NonClosingFile {
    file: ManuallyDrop<std::fs::File>
}

impl FromRawFd for NonClosingFile {
    unsafe fn from_raw_fd(fd: RawFd) -> Self {
        Self{
            file: ManuallyDrop::new(std::fs::File::from_raw_fd(fd))
        }
    }
}

impl Drop for NonClosingFile {
    fn drop(&mut self) {
        // Safety: we must never use self.file again. It's OK, we won't,
        // we only get dropped once.
        unsafe { ManuallyDrop::take(&mut self.file) }.into_raw_fd();
    }
}

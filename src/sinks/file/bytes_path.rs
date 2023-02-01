//! Fun little hack around bytes and OsStr

use std::path::Path;

use bytes::Bytes;

#[derive(Debug, Clone)]
pub struct BytesPath {
    #[cfg(unix)]
    path: Bytes,
    #[cfg(windows)]
    path: std::path::PathBuf,
}

impl BytesPath {
    #[cfg(unix)]
    pub fn new(path: Bytes) -> Self {
        Self { path }
    }
    #[cfg(windows)]
    pub fn new(path: Bytes) -> Self {
        let utf8_string = String::from_utf8_lossy(&path[..]);
        let path = std::path::PathBuf::from(utf8_string.as_ref());
        Self { path }
    }
}

impl AsRef<Path> for BytesPath {
    #[cfg(unix)]
    fn as_ref(&self) -> &Path {
        use std::os::unix::ffi::OsStrExt;
        let os_str = std::ffi::OsStr::from_bytes(&self.path);
        Path::new(os_str)
    }
    #[cfg(windows)]
    fn as_ref(&self) -> &Path {
        &self.path.as_ref()
    }
}

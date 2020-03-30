use futures01::Poll;
use std::fmt::{self, Debug, Formatter};
use std::io::{self, Read, Write};
use tokio01::io::{AsyncRead, AsyncWrite};

/// A type wrapper for objects that can exist in either a raw state or
/// wrapped by TLS handling.
pub enum MaybeTls<R, T> {
    Raw(R),
    Tls(T),
}

impl<R, T> MaybeTls<R, T> {
    pub fn is_raw(&self) -> bool {
        matches!(self, Self::Raw(_))
    }

    pub fn is_tls(&self) -> bool {
        matches!(self, Self::Tls(_))
    }

    pub fn raw(&self) -> Option<&R> {
        match self {
            Self::Raw(raw) => Some(&raw),
            Self::Tls(_) => None,
        }
    }

    pub fn tls(&self) -> Option<&T> {
        match self {
            Self::Raw(_) => None,
            Self::Tls(tls) => Some(&tls),
        }
    }
}

impl<T> From<Option<T>> for MaybeTls<(), T> {
    fn from(tls: Option<T>) -> Self {
        match tls {
            Some(tls) => Self::Tls(tls),
            None => Self::Raw(()),
        }
    }
}

// Conditionally implement Clone for Clonable types
impl<R: Clone, T: Clone> Clone for MaybeTls<R, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Raw(raw) => Self::Raw(raw.clone()),
            Self::Tls(tls) => Self::Tls(tls.clone()),
        }
    }
}

// Conditionally implement Debug for Debugable types
impl<R: Debug, T: Debug> Debug for MaybeTls<R, T> {
    fn fmt(&self, fmt: &mut Formatter) -> fmt::Result {
        match self {
            Self::Raw(raw) => write!(fmt, "MaybeTls::Raw({:?})", raw),
            Self::Tls(tls) => write!(fmt, "MaybeTls::Tls({:?})", tls),
        }
    }
}

impl<R: Read, T: Read> Read for MaybeTls<R, T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Tls(s) => s.read(buf),
            Self::Raw(s) => s.read(buf),
        }
    }
}

impl<R: AsyncRead, T: AsyncRead> AsyncRead for MaybeTls<R, T> {}

impl<R: Write, T: Write> Write for MaybeTls<R, T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Self::Tls(s) => s.write(buf),
            Self::Raw(s) => s.write(buf),
        }
    }

    fn flush(&mut self) -> io::Result<()> {
        match self {
            Self::Tls(s) => s.flush(),
            Self::Raw(s) => s.flush(),
        }
    }
}

impl<R: AsyncWrite, T: AsyncWrite> AsyncWrite for MaybeTls<R, T> {
    fn shutdown(&mut self) -> Poll<(), io::Error> {
        match self {
            Self::Tls(s) => s.shutdown(),
            Self::Raw(s) => s.shutdown(),
        }
    }
}

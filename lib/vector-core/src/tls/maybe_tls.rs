use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

use pin_project::pin_project;
use tokio::io::{self, AsyncRead, AsyncWrite, ReadBuf};

/// A type wrapper for objects that can exist in either a raw state or
/// wrapped by TLS handling.
#[pin_project(project = MaybeTlsProj)]
pub enum MaybeTls<R, T> {
    Raw(#[pin] R),
    Tls(#[pin] T),
}

impl<R, T> MaybeTls<R, T> {
    pub const fn is_raw(&self) -> bool {
        matches!(self, Self::Raw(_))
    }

    pub const fn is_tls(&self) -> bool {
        matches!(self, Self::Tls(_))
    }

    pub const fn raw(&self) -> Option<&R> {
        match self {
            Self::Raw(raw) => Some(raw),
            Self::Tls(_) => None,
        }
    }

    pub const fn tls(&self) -> Option<&T> {
        match self {
            Self::Raw(_) => None,
            Self::Tls(tls) => Some(tls),
        }
    }
}

impl<O> From<Option<O>> for MaybeTls<(), O> {
    fn from(tls: Option<O>) -> Self {
        match tls {
            Some(tls) => Self::Tls(tls),
            None => Self::Raw(()),
        }
    }
}

// Conditionally implement Clone for Cloneable types
impl<R: Clone, T: Clone> Clone for MaybeTls<R, T> {
    fn clone(&self) -> Self {
        match self {
            Self::Raw(raw) => Self::Raw(raw.clone()),
            Self::Tls(tls) => Self::Tls(tls.clone()),
        }
    }
}

// Conditionally implement Debug for Debuggable types
impl<R: fmt::Debug, T: fmt::Debug> fmt::Debug for MaybeTls<R, T> {
    fn fmt(&self, fmt: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Raw(raw) => write!(fmt, "MaybeTls::Raw({raw:?})"),
            Self::Tls(tls) => write!(fmt, "MaybeTls::Tls({tls:?})"),
        }
    }
}

impl<R: AsyncRead, T: AsyncRead> AsyncRead for MaybeTls<R, T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeTlsProj::Tls(s) => s.poll_read(cx, buf),
            MaybeTlsProj::Raw(s) => s.poll_read(cx, buf),
        }
    }
}

impl<R: AsyncWrite, T: AsyncWrite> AsyncWrite for MaybeTls<R, T> {
    fn poll_write(self: Pin<&mut Self>, cx: &mut Context, buf: &[u8]) -> Poll<io::Result<usize>> {
        match self.project() {
            MaybeTlsProj::Tls(s) => s.poll_write(cx, buf),
            MaybeTlsProj::Raw(s) => s.poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeTlsProj::Tls(s) => s.poll_flush(cx),
            MaybeTlsProj::Raw(s) => s.poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context) -> Poll<io::Result<()>> {
        match self.project() {
            MaybeTlsProj::Tls(s) => s.poll_shutdown(cx),
            MaybeTlsProj::Raw(s) => s.poll_shutdown(cx),
        }
    }
}

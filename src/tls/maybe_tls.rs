use bytes::{Buf, BufMut};
use pin_project::pin_project;
use std::{
    fmt,
    mem::MaybeUninit,
    pin::Pin,
    task::{Context, Poll},
};
use tokio::io::{self, AsyncRead, AsyncWrite};

/// A type wrapper for objects that can exist in either a raw state or
/// wrapped by TLS handling.
#[pin_project(project = MaybeTlsProj)]
pub enum MaybeTls<R, T> {
    Raw(#[pin] R),
    Tls(#[pin] T),
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
            Self::Raw(raw) => write!(fmt, "MaybeTls::Raw({:?})", raw),
            Self::Tls(tls) => write!(fmt, "MaybeTls::Tls({:?})", tls),
        }
    }
}

impl<R: AsyncRead, T: AsyncRead> AsyncRead for MaybeTls<R, T> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        match self.project() {
            MaybeTlsProj::Tls(s) => s.poll_read(cx, buf),
            MaybeTlsProj::Raw(s) => s.poll_read(cx, buf),
        }
    }

    unsafe fn prepare_uninitialized_buffer(&self, buf: &mut [MaybeUninit<u8>]) -> bool {
        match self {
            MaybeTls::Tls(s) => s.prepare_uninitialized_buffer(buf),
            MaybeTls::Raw(s) => s.prepare_uninitialized_buffer(buf),
        }
    }

    fn poll_read_buf<B: BufMut>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        match self.project() {
            MaybeTlsProj::Tls(s) => s.poll_read_buf(cx, buf),
            MaybeTlsProj::Raw(s) => s.poll_read_buf(cx, buf),
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

    fn poll_write_buf<B: Buf>(
        self: Pin<&mut Self>,
        cx: &mut Context,
        buf: &mut B,
    ) -> Poll<io::Result<usize>> {
        match self.project() {
            MaybeTlsProj::Tls(s) => s.poll_write_buf(cx, buf),
            MaybeTlsProj::Raw(s) => s.poll_write_buf(cx, buf),
        }
    }
}

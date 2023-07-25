//! `sftp` sink.
//!
//! `sftp` SFTP, or Secure File Transfer Protocol, is a network protocol used for
//! securely transferring files over the internet. It operates over the Secure
//! Shell (SSH) data stream, providing secure file transfer by both encrypting
//! the data and maintaining the integrity of the transfer. SFTP also supports
//! file management operations like moving and deleting files on the server, unlike FTP.
//!
//! For more information, please refer to:
//!
//! - [sftp(1) — Linux manual page](https://man7.org/linux/man-pages/man1/sftp.1.html)
//!
//! `sftp` is an OpenDal based services. This mod itself only provide config to build an
//! [`crate::sinks::opendal_common::OpenDalSink`]. All real implement are powered by
//! [`crate::sinks::opendal_common::OpenDalSink`].

mod config;
pub use config::SftpConfig;

#[cfg(test)]
mod test;

use nix::fcntl::{FcntlArg, SealFlag};
use nix::sys::socket::{ControlMessage, MsgFlags};
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

/// A writer for journald that sends log messages over a Unix domain socket.
/// For protocol details, see [this link](https://systemd.io/JOURNAL_NATIVE_PROTOCOL/)
pub struct JournaldWriter {
    socket: tokio::net::UnixDatagram,
    buf: Vec<u8>,
}

impl JournaldWriter {
    pub fn new(journald_path: impl AsRef<Path>) -> io::Result<Self> {
        let socket = tokio::net::UnixDatagram::unbound()?;
        socket.connect(journald_path)?;
        let writer = Self {
            socket,
            buf: vec![],
        };
        Ok(writer)
    }

    /// Add a string field to the buffer.
    pub fn add_str(&mut self, key: &str, value: &str) {
        self.write_with_length(key, |w| {
            w.buf.extend_from_slice(value.as_bytes());
        });
    }

    /// Add a field with arbitrary bytes to the buffer.
    pub fn add_bytes(&mut self, key: &str, value: &[u8]) {
        self.write_with_length(key, |w| {
            w.buf.extend_from_slice(value);
        });
    }

    /// Write the buffered data to journald.
    /// Returns the number of bytes sent.
    pub async fn write(&mut self) -> io::Result<usize> {
        if self.buf.is_empty() {
            return Ok(0);
        }
        let bytes_sent = self.send_payload(&self.buf).await?;
        // Clear the buffer after sending
        // We could also keep the buffer for reuse, but by doing this we ensure that
        // we don't allocate too much memory for long time in case of rare large payloads.
        self.buf = vec![];
        Ok(bytes_sent)
    }

    /// Send the payload to journald.
    /// If the payload is too large, it will attempt to send it via a memfd.
    /// Returns the number of bytes sent.
    async fn send_payload(&self, payload: &[u8]) -> io::Result<usize> {
        self.socket.send(payload).await.or_else(|error| {
            if Some(nix::libc::EMSGSIZE) == error.raw_os_error() {
                self.send_with_memfd(payload)
            } else {
                Err(error)
            }
        })
    }

    /// Send the payload using a memfd if the payload is too large for a direct send.
    /// This method uses a blocking call to write the payload to a memfd.
    fn send_with_memfd(&self, payload: &[u8]) -> io::Result<usize> {
        // If the payload is too large, we should try to send it via a memfd
        // This method is described in the journald protocol: https://systemd.io/JOURNAL_NATIVE_PROTOCOL/
        let memfd = nix::sys::memfd::memfd_create(
            c"journald_payload",
            nix::sys::memfd::MemFdCreateFlag::MFD_ALLOW_SEALING,
        )?;

        // Write the payload to the memfd
        let written = nix::unistd::write(memfd, payload)?;
        if written != payload.len() {
            return Err(io::Error::other("Failed to write all data to memfd"));
        }

        // Seal the memfd as required by journald protocol
        nix::fcntl::fcntl(memfd, FcntlArg::F_ADD_SEALS(SealFlag::all()))?;

        // Send the memfd file descriptor to journald
        let scm = &[memfd];
        let cmsgs = [ControlMessage::ScmRights(scm)];
        nix::sys::socket::sendmsg::<()>(
            self.socket.as_raw_fd(),
            &[],
            &cmsgs,
            MsgFlags::empty(),
            None,
        )?;

        Ok(written)
    }

    /// Append a sanitized and length-encoded field into the buffer.
    fn write_with_length(&mut self, key: &str, write_cb: impl FnOnce(&mut Self)) {
        self.sanitize_key(key);
        self.buf.push(b'\n');
        self.buf.extend_from_slice(&[0; 8]); // Length tag, to be populated after writing the value
        let start = self.buf.len();
        write_cb(self);
        let end = self.buf.len();
        self.buf[start - 8..start].copy_from_slice(&((end - start) as u64).to_le_bytes());
        self.buf.push(b'\n');
    }

    /// Sanitize a key and convert it to uppercase.
    fn sanitize_key(&mut self, key: &str) {
        self.buf.extend(
            key.bytes()
                .map(|c| match c {
                    // As per journald protocol, '=' and '\n' are illegal in keys so we replace them with '_'.
                    b'=' | b'\n' => b'_',
                    // '.' is ignored in keys, so we replace it with '_'.
                    b'.' => b'_',
                    _ => c,
                })
                .filter(|&c| c == b'_' || char::from(c).is_ascii_alphanumeric())
                .map(|c| char::from(c).to_ascii_uppercase() as u8),
        );
    }
}

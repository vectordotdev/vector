use bytes::{BufMut, BytesMut};
use memchr::memchr;
use std::io::{self, Read};

pub mod console;

pub struct ReaderSource<T> {
    inner: T,
    buf: BytesMut,
}

impl<T: Read> ReaderSource<T> {
    pub fn new(inner: T) -> Self {
        let buf = BytesMut::new();
        Self { inner, buf }
    }

    pub fn pull(&mut self) -> io::Result<BytesMut> {
        loop {
            if let Some(pos) = memchr(b'\n', &self.buf) {
                let mut line = self.buf.split_to(pos + 1);
                line.split_off(pos);
                return Ok(line);
            } else {
                let n = self.fill_buf()?;
                if n == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::UnexpectedEof,
                        "reader closed with no newline",
                    ));
                }
            }
        }
    }

    fn fill_buf(&mut self) -> io::Result<usize> {
        self.buf.reserve(1024 * 10);
        unsafe {
            let n = self.inner.read(self.buf.bytes_mut())?;
            self.buf.advance_mut(n);
            Ok(n)
        }
    }
}

#[cfg(test)]
mod test {
    use super::ReaderSource;
    use std::io::{self, Cursor};

    #[test]
    fn reader_source_works() {
        let src = Cursor::new("hello world\n".repeat(10));
        let mut rdr = ReaderSource::new(src);
        for _ in 0..10 {
            assert_eq!(rdr.pull().unwrap(), "hello world");
        }
    }

    #[test]
    fn reader_source_works_for_really_long_lines() {
        let line = "a".repeat(1024 * 100);
        let src = Cursor::new(format!("{}\n", line));
        let mut rdr = ReaderSource::new(src);
        assert_eq!(rdr.pull().unwrap(), line);
    }

    #[test]
    fn reader_source_returns_eof_if_no_newline() {
        let line = "a".repeat(1024 * 100);
        let src = Cursor::new(format!("{}", line));
        let mut rdr = ReaderSource::new(src);
        assert_eq!(rdr.pull().unwrap_err().kind(), io::ErrorKind::UnexpectedEof);
    }
}

//! Resumable, byte-budgeted framing for tailing files.

use std::io;

use bstr::Finder;
use bytes::{Buf, Bytes, BytesMut};
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

use crate::FilePosition;

/// Why a bounded read stopped. Exhausting a budget is not evidence of EOF.
#[derive(Debug, PartialEq, Eq)]
pub enum ReadOutcome {
    Line,
    Yield,
    Eof,
}

/// Retains framing state across read budgets and temporary EOFs.
///
/// The caller's buffer holds at most `max_size` payload bytes. Oversized records
/// are discarded without retaining samples; only a possible delimiter prefix is
/// retained while skipping them.
pub struct BoundedLineReader {
    delimiter: Bytes,
    max_size: usize,
    discarding: bool,
    prefix: BytesMut,
}

impl BoundedLineReader {
    pub fn new(delimiter: Bytes, max_size: usize) -> Self {
        Self {
            delimiter,
            max_size,
            discarding: false,
            prefix: BytesMut::new(),
        }
    }

    /// Forget framing state after the underlying file is rewound or replaced.
    pub fn reset(&mut self) {
        self.discarding = false;
        self.prefix.clear();
    }

    /// Consume at most `budget` bytes, including delimiters and discarded data.
    /// On `Line`, the caller must take the completed payload from `buf`.
    ///
    /// # Errors
    /// Returns an error for an empty delimiter or a non-interrupted reader error.
    pub async fn read<R: AsyncBufRead + Unpin + ?Sized>(
        &mut self,
        reader: &mut R,
        position: &mut FilePosition,
        buf: &mut BytesMut,
        budget: usize,
    ) -> io::Result<ReadOutcome> {
        if self.delimiter.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty line delimiter",
            ));
        }
        let finder = Finder::new(&self.delimiter);
        let mut remaining = budget;
        while remaining > 0 {
            let available = match reader.fill_buf().await {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => return Err(error),
            };
            if available.is_empty() {
                return Ok(ReadOutcome::Eof);
            }
            let available = &available[..available.len().min(remaining)];
            let (used, complete) = if self.prefix.is_empty() {
                if let Some(index) = finder.find(available) {
                    append_payload(
                        buf,
                        &mut self.discarding,
                        self.max_size,
                        &available[..index],
                    );
                    (index + self.delimiter.len(), true)
                } else {
                    let suffix = (1..self.delimiter.len().min(available.len() + 1))
                        .rev()
                        .find(|&size| available.ends_with(&self.delimiter[..size]))
                        .unwrap_or(0);
                    let split = available.len() - suffix;
                    append_payload(
                        buf,
                        &mut self.discarding,
                        self.max_size,
                        &available[..split],
                    );
                    self.prefix.extend_from_slice(&available[split..]);
                    (available.len(), false)
                }
            } else {
                // Resolve boundary-spanning delimiters before returning to bulk
                // searching. Retain the longest suffix that can still match.
                self.prefix.extend_from_slice(&available[..1]);
                if self.prefix.as_ref() == self.delimiter.as_ref() {
                    self.prefix.clear();
                    (1, true)
                } else {
                    let suffix = (1..self.delimiter.len().min(self.prefix.len() + 1))
                        .rev()
                        .find(|&size| self.prefix.ends_with(&self.delimiter[..size]))
                        .unwrap_or(0);
                    let confirmed = self.prefix.len() - suffix;
                    append_payload(
                        buf,
                        &mut self.discarding,
                        self.max_size,
                        &self.prefix[..confirmed],
                    );
                    self.prefix.advance(confirmed);
                    (1, false)
                }
            };
            reader.consume(used);
            *position += used as u64;
            remaining -= used;
            if complete {
                if self.discarding {
                    self.discarding = false;
                    buf.clear();
                } else {
                    return Ok(ReadOutcome::Line);
                }
            }
        }
        Ok(ReadOutcome::Yield)
    }
}

fn append_payload(buf: &mut BytesMut, discarding: &mut bool, max_size: usize, bytes: &[u8]) {
    if !*discarding {
        if bytes.len() > max_size.saturating_sub(buf.len()) {
            buf.clear();
            *discarding = true;
        } else {
            buf.extend_from_slice(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use tokio::io::BufReader;

    use super::*;

    #[tokio::test]
    async fn budgets_preserve_delimiters_and_discard_state() {
        for delimiter in ["\n", "\r\n", "aba", "abab", "abcab"] {
            for budget in [1, 2, 3, 7, 64] {
                for capacity in [1, 2, 5, 8192] {
                    let input =
                        format!("ok{delimiter}{}{delimiter}fin{delimiter}", "x".repeat(128));
                    let mut reader =
                        BufReader::with_capacity(capacity, Cursor::new(input.as_bytes()));
                    let mut state = BoundedLineReader::new(Bytes::from(delimiter), 3);
                    let mut position = 0;
                    let mut buf = BytesMut::new();
                    let mut lines = Vec::new();
                    loop {
                        let before = position;
                        let result = state
                            .read(&mut reader, &mut position, &mut buf, budget)
                            .await
                            .unwrap();
                        assert!(position - before <= budget as u64);
                        assert!(buf.len() <= 3);
                        assert!(state.prefix.len() < delimiter.len());
                        match result {
                            ReadOutcome::Line => lines.push(buf.split().freeze()),
                            ReadOutcome::Yield => assert!(position > before),
                            ReadOutcome::Eof => break,
                        }
                    }
                    assert_eq!(position, input.len() as u64);
                    assert_eq!(
                        lines,
                        [Bytes::from_static(b"ok"), Bytes::from_static(b"fin")]
                    );
                }
            }
        }
    }

    #[tokio::test]
    async fn partial_delimiters_survive_temporary_eof_and_mismatches() {
        for prefix in ["ok", "xxxxxxxx"] {
            let mut reader =
                BufReader::with_capacity(1, Cursor::new(format!("{prefix}\r").into_bytes()));
            let mut state = BoundedLineReader::new(Bytes::from_static(b"\r\n"), 3);
            let mut position = 0;
            let mut buf = BytesMut::new();
            assert_eq!(
                state
                    .read(&mut reader, &mut position, &mut buf, 64)
                    .await
                    .unwrap(),
                ReadOutcome::Eof
            );
            reader.get_mut().get_mut().extend_from_slice(b"\nfin\r\n");
            assert_eq!(
                state
                    .read(&mut reader, &mut position, &mut buf, 64)
                    .await
                    .unwrap(),
                ReadOutcome::Line
            );
            assert_eq!(
                &buf[..],
                if prefix == "ok" {
                    b"ok".as_slice()
                } else {
                    b"fin".as_slice()
                }
            );
        }
        let mut reader = BufReader::with_capacity(1, Cursor::new(b"abxababfinabab"));
        let mut state = BoundedLineReader::new(Bytes::from_static(b"abab"), 3);
        let mut position = 0;
        let mut buf = BytesMut::new();
        assert_eq!(
            state
                .read(&mut reader, &mut position, &mut buf, 64)
                .await
                .unwrap(),
            ReadOutcome::Line
        );
        assert_eq!(&buf[..], b"abx");
        buf.clear();
        assert_eq!(
            state
                .read(&mut reader, &mut position, &mut buf, 64)
                .await
                .unwrap(),
            ReadOutcome::Line
        );
        assert_eq!(&buf[..], b"fin");
    }

    #[tokio::test]
    async fn huge_unterminated_record_uses_bounded_turns_and_memory() {
        let input = format!("{}\nend\n", "x".repeat(1024 * 1024));
        let mut reader = BufReader::new(Cursor::new(input.as_bytes()));
        let mut state = BoundedLineReader::new(Bytes::from_static(b"\n"), 8);
        let mut position = 0;
        let mut buf = BytesMut::new();
        for _ in 0..1024 {
            let before = position;
            assert_eq!(
                state
                    .read(&mut reader, &mut position, &mut buf, 1024)
                    .await
                    .unwrap(),
                ReadOutcome::Yield
            );
            assert_eq!(position - before, 1024);
            assert!(buf.len() <= 8);
            assert!(state.prefix.is_empty());
        }
        assert_eq!(
            state
                .read(&mut reader, &mut position, &mut buf, 1024)
                .await
                .unwrap(),
            ReadOutcome::Line
        );
        assert_eq!(&buf[..], b"end");
        assert_eq!(position, input.len() as u64);
    }
}

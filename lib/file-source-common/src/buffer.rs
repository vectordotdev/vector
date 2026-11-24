use crate::FilePosition;
use std::{cmp::min, io};

use bstr::Finder;
use bytes::BytesMut;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

pub struct ReadResult {
    pub successfully_read: Option<usize>,
    pub discarded_for_size_and_truncated: Vec<BytesMut>,
}

/// Persistent line-reading state across `read_until_with_max_size_stateful` calls,
/// preserving partial multi-byte delimiters, oversized-line discard state, and the
/// true starting byte offset of an in-progress line across EOF boundaries.
#[derive(Debug, Default)]
pub struct LineBufferState {
    pub buf: BytesMut,
    partial_delim: BytesMut,
    discarding: bool,
    line_start_offset: Option<FilePosition>,
}

impl LineBufferState {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if no buffered line bytes or partial delimiter bytes are held.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty() && self.partial_delim.is_empty()
    }

    /// Takes the completed line bytes and its starting byte offset, resetting per-line state.
    pub fn take_line(&mut self, fallback_offset: FilePosition) -> (FilePosition, bytes::Bytes) {
        let offset = self.line_start_offset.take().unwrap_or(fallback_offset);
        (offset, self.buf.split().freeze())
    }

    /// Flushes any remaining partial line bytes (including an incomplete trailing
    /// delimiter) when a file has been deleted/closed, resetting all line state.
    pub fn take_remainder(
        &mut self,
        fallback_offset: FilePosition,
    ) -> (FilePosition, bytes::Bytes) {
        let offset = self.line_start_offset.take().unwrap_or(fallback_offset);
        if self.discarding {
            self.discarding = false;
            self.buf.clear();
            self.partial_delim.clear();
            return (offset, bytes::Bytes::new());
        }
        if !self.partial_delim.is_empty() {
            self.buf.extend_from_slice(&self.partial_delim);
            self.partial_delim.clear();
        }
        (offset, self.buf.split().freeze())
    }
}

fn longest_delim_prefix_suffix(slice: &[u8], delim: &[u8]) -> usize {
    let max_prefix = delim.len().saturating_sub(1).min(slice.len());
    for prefix_len in (1..=max_prefix).rev() {
        if slice.ends_with(&delim[..prefix_len]) {
            return prefix_len;
        }
    }
    0
}

fn check_max_size(state: &mut LineBufferState, max_size: usize, discarded: &mut Vec<BytesMut>) {
    if !state.discarding && state.buf.len() > max_size {
        // keep only the first <1k bytes to make sure we can actually emit a usable error
        let length_to_keep = min(1000, max_size);
        let mut truncated: BytesMut = BytesMut::zeroed(length_to_keep);
        truncated.copy_from_slice(&state.buf[0..length_to_keep]);
        discarded.push(truncated);
        state.discarding = true;
    }
}

enum PartialDelimStep {
    /// Completed a delimiter spanning the boundary, consuming `usize` bytes from `available`.
    Matched(usize),
    /// Consumed the entire `available` slice (`< delim.len() - 1`) without completing `delim`.
    ConsumedAll(usize),
    /// No delimiter started in `partial_delim`; `partial_delim` was flushed to `state.buf`
    /// and caller should scan `available` normally.
    Flushed,
}

fn resolve_partial_delim(
    available: &[u8],
    delim: &[u8],
    delim_finder: &Finder<'_>,
    state: &mut LineBufferState,
    max_size: usize,
    discarded: &mut Vec<BytesMut>,
) -> PartialDelimStep {
    let delim_len = delim.len();
    let p_len = state.partial_delim.len();
    let take = available.len().min(delim_len.saturating_sub(1));
    let mut bridge = Vec::with_capacity(p_len + take);
    bridge.extend_from_slice(&state.partial_delim);
    bridge.extend_from_slice(&available[..take]);

    if let Some(start) = delim_finder.find(&bridge) {
        if !state.discarding && start > 0 {
            state.buf.extend_from_slice(&state.partial_delim[..start]);
            check_max_size(state, max_size, discarded);
        }
        let consume_from_avail = start + delim_len - p_len;
        state.partial_delim.clear();
        return PartialDelimStep::Matched(consume_from_avail);
    }

    if available.len() < delim_len.saturating_sub(1) {
        let keep = longest_delim_prefix_suffix(&bridge, delim);
        let flush_len = bridge.len() - keep;
        if !state.discarding && flush_len > 0 {
            state.buf.extend_from_slice(&bridge[..flush_len]);
            check_max_size(state, max_size, discarded);
        }
        state.partial_delim.clear();
        state.partial_delim.extend_from_slice(&bridge[flush_len..]);
        return PartialDelimStep::ConsumedAll(available.len());
    }

    if !state.discarding {
        state.buf.extend_from_slice(&state.partial_delim);
        check_max_size(state, max_size, discarded);
    }
    state.partial_delim.clear();
    PartialDelimStep::Flushed
}

/// Stateful version of [`read_until_with_max_size`] that preserves `partial_delim`,
/// `discarding`, and `line_start_offset` across EOF boundaries.
///
/// # Errors
/// Returns an error if reading from `reader` fails, except interrupted reads,
/// which are retried.
pub async fn read_until_with_max_size_stateful<'a, R: AsyncBufRead + ?Sized + Unpin>(
    reader: &'a mut R,
    position: &'a mut FilePosition,
    delim: &'a [u8],
    state: &'a mut LineBufferState,
    max_size: usize,
) -> io::Result<ReadResult> {
    let mut total_read = 0;
    let delim_finder = Finder::new(delim);
    let delim_len = delim.len();
    let mut discarded_for_size_and_truncated = Vec::new();

    loop {
        let available: &[u8] = match reader.fill_buf().await {
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        if available.is_empty() {
            return Ok(ReadResult {
                successfully_read: None,
                discarded_for_size_and_truncated,
            });
        }

        if state.line_start_offset.is_none() {
            state.line_start_offset = Some(*position);
        }

        if !state.partial_delim.is_empty() {
            match resolve_partial_delim(
                available,
                delim,
                &delim_finder,
                state,
                max_size,
                &mut discarded_for_size_and_truncated,
            ) {
                PartialDelimStep::Matched(used) => {
                    reader.consume(used);
                    *position += used as u64;
                    total_read += used;
                    if state.discarding {
                        state.discarding = false;
                        state.buf.clear();
                        state.line_start_offset = None;
                        continue;
                    }
                    return Ok(ReadResult {
                        successfully_read: Some(total_read),
                        discarded_for_size_and_truncated,
                    });
                }
                PartialDelimStep::ConsumedAll(used) => {
                    reader.consume(used);
                    *position += used as u64;
                    total_read += used;
                    continue;
                }
                PartialDelimStep::Flushed => {}
            }
        }

        let (done, used) = if let Some(i) = delim_finder.find(available) {
            if !state.discarding {
                state.buf.extend_from_slice(&available[..i]);
            }
            (true, i + delim_len)
        } else {
            let partial_match_len = if delim_len > 1 {
                longest_delim_prefix_suffix(available, delim)
            } else {
                0
            };
            let bytes_to_copy = available.len() - partial_match_len;
            if !state.discarding && bytes_to_copy > 0 {
                state.buf.extend_from_slice(&available[..bytes_to_copy]);
            }
            if partial_match_len > 0 {
                state.partial_delim.clear();
                state
                    .partial_delim
                    .extend_from_slice(&available[bytes_to_copy..]);
            }
            (false, available.len())
        };

        reader.consume(used);
        *position += used as u64;
        total_read += used;

        check_max_size(state, max_size, &mut discarded_for_size_and_truncated);

        if done {
            if state.discarding {
                state.discarding = false;
                state.buf.clear();
                state.line_start_offset = None;
            } else {
                return Ok(ReadResult {
                    successfully_read: Some(total_read),
                    discarded_for_size_and_truncated,
                });
            }
        }
    }
}

/// Read up to `max_size` bytes from `reader`, splitting by `delim`
///
/// The function reads up to `max_size` bytes from `reader`, splitting the input
/// by `delim`. If a `delim` is not found in `reader` before `max_size` bytes
/// are read then the reader is polled until `delim` is found and the results
/// are discarded. Else, the result is written into `buf`.
///
/// # Errors
///
/// Returns an error if reading from `reader` fails, except interrupted reads,
/// which are retried.
pub async fn read_until_with_max_size<'a, R: AsyncBufRead + ?Sized + Unpin>(
    reader: &'a mut R,
    position: &'a mut FilePosition,
    delim: &'a [u8],
    buf: &'a mut BytesMut,
    max_size: usize,
) -> io::Result<ReadResult> {
    let mut state = LineBufferState {
        buf: std::mem::take(buf),
        partial_delim: BytesMut::new(),
        discarding: false,
        line_start_offset: None,
    };
    let result =
        read_until_with_max_size_stateful(reader, position, delim, &mut state, max_size).await;
    if !state.partial_delim.is_empty() && !state.discarding {
        state.buf.extend_from_slice(&state.partial_delim);
    }
    *buf = state.buf;
    result
}

#[cfg(test)]
mod test {
    use std::{io::Cursor, num::NonZeroU8, ops::Range};

    use bytes::{BufMut, BytesMut};
    use quickcheck::{QuickCheck, TestResult};
    use tokio::io::BufReader;

    use super::read_until_with_max_size;
    use crate::buffer::ReadResult;

    async fn qc_inner(chunks: Vec<Vec<u8>>, delim: u8, max_size: NonZeroU8) -> TestResult {
        // `DelimDetails` describes the nature of each delimiter found in the
        // `chunks`.
        #[derive(Clone)]
        struct DelimDetails {
            /// Index in `global_data`, absolute offset
            global_index: usize,
            /// Index in each `chunk`, relative offset
            interior_index: usize,
            /// Whether this delimiter was within `max_size` of its previous
            /// peer
            within_max_size: bool,
            /// Which chunk in `chunks` this delimiter was found in
            chunk_index: usize,
            /// The range of bytes that this delimiter bounds with its previous
            /// peer
            byte_range: Range<usize>,
        }

        // The `global_data` is the view of `chunks` as a single contiguous
        // block of memory. Where `chunks` simulates what happens when bytes are
        // fitfully available `global_data` is the view of all chunks assembled
        // after every byte is available.
        let mut global_data = BytesMut::new();

        // Move through the `chunks` and discover at what positions an instance
        // of `delim` exists in the chunk stream and whether that `delim` is
        // more than `max_size` bytes away from the previous `delim`. This loop
        // constructs the `facts` vector that holds `DelimDetails` instances and
        // also populates `global_data`.
        let mut facts: Vec<DelimDetails> = Vec::new();
        let mut global_index: usize = 0;
        let mut previous_offset: usize = 0;
        for (i, chunk) in chunks.iter().enumerate() {
            for (interior_index, byte) in chunk.iter().enumerate() {
                global_data.put_u8(*byte);
                if *byte == delim {
                    let span = global_index - previous_offset;
                    let within_max_size = span <= max_size.get() as usize;
                    facts.push(DelimDetails {
                        global_index,
                        within_max_size,
                        chunk_index: i,
                        interior_index,
                        byte_range: (previous_offset..global_index),
                    });
                    previous_offset = global_index + 1;
                }
                global_index += 1;
            }
        }

        // Our model is only concerned with the first valid delimiter in the
        // chunk stream. As such, discover that first valid delimiter and record
        // it specially.
        let mut first_delim: Option<DelimDetails> = None;
        for fact in &facts {
            if fact.within_max_size {
                first_delim = Some(fact.clone());
                break;
            }
        }

        let mut position = 0;
        let mut buffer = BytesMut::with_capacity(max_size.get() as usize);
        // NOTE: The delimiter may be multiple bytes wide but for the purpose of
        // this model a single byte does well enough.
        let delimiter: [u8; 1] = [delim];
        for (idx, chunk) in chunks.iter().enumerate() {
            let mut reader = BufReader::new(Cursor::new(&chunk));

            match read_until_with_max_size(
                &mut reader,
                &mut position,
                &delimiter,
                &mut buffer,
                max_size.get() as usize,
            )
            .await
            .unwrap()
            {
                ReadResult {
                    successfully_read: None,
                    discarded_for_size_and_truncated: _,
                } => {
                    // Subject only returns None if this is the last chunk _and_
                    // the chunk did not contain a delimiter _or_ the delimiter
                    // was outside the max_size range _or_ the current chunk is empty.
                    let has_valid_delimiter = facts
                        .iter()
                        .any(|details| (details.chunk_index == idx) && details.within_max_size);
                    assert!(chunk.is_empty() || !has_valid_delimiter);
                }
                ReadResult {
                    successfully_read: Some(total_read),
                    discarded_for_size_and_truncated: _,
                } => {
                    // Now that the function has returned we confirm that the
                    // returned details match our `first_delim` and also that
                    // the `buffer` is populated correctly.
                    assert!(first_delim.is_some());
                    assert_eq!(
                        first_delim.clone().unwrap().global_index + 1,
                        usize::try_from(position).expect("position fits in the input buffer")
                    );
                    assert_eq!(first_delim.clone().unwrap().interior_index + 1, total_read);
                    assert_eq!(
                        buffer.get(..),
                        global_data.get(first_delim.unwrap().byte_range)
                    );
                    break;
                }
            }
        }

        TestResult::passed()
    }

    #[tokio::test]
    async fn qc_read_until_with_max_size() {
        // The `read_until_with_max` function is intended to be called
        // multiple times until it returns Ok(Some(usize)). The function
        // should never return error in this test. If the return is None we
        // will call until it is not. Once return is Some the interior value
        // should be the position of the first delim in the buffer, unless
        // that delim is past the max_size barrier in which case it will be
        // the position of the first delim that is within some multiple of
        // max_size.
        //
        // I think I will adjust the function to have a richer return
        // type. This will help in the transition to async.
        fn inner(chunks: Vec<Vec<u8>>, delim: u8, max_size: NonZeroU8) -> TestResult {
            let handle = tokio::runtime::Handle::current();
            handle.block_on(qc_inner(chunks, delim, max_size));
            TestResult::passed()
        }

        tokio::task::spawn_blocking(|| {
            QuickCheck::new()
                .tests(1_000)
                .max_tests(2_000)
                .quickcheck(inner as fn(Vec<Vec<u8>>, u8, NonZeroU8) -> TestResult);
        })
        .await
        .unwrap();
    }

    /// Generic test helper that tests delimiter splits across buffer boundaries
    /// for any delimiter length. This function:
    /// 1. Creates test data with delimiters positioned to split at buffer boundaries
    /// 2. Tests multiple iterations to ensure state tracking works correctly
    /// 3. Verifies all lines are correctly separated without merging
    async fn test_delimiter_boundary_split_helper(delimiter: &[u8], num_lines: usize) {
        let delimiter_len = delimiter.len();

        // Use a buffer capacity that will force splits
        // We'll position delimiters to split at this boundary
        let buffer_capacity = 10;

        // Build test data where each delimiter is positioned to split across buffer boundary
        // Strategy: For each line, calculate position so delimiter starts at boundary - (delimiter_len - 1)
        let mut data = Vec::new();
        let mut expected_lines = Vec::new();

        for i in 0..num_lines {
            // Create line content that positions the delimiter to split at buffer boundary
            // We want the delimiter to straddle a buffer_capacity boundary

            // Calculate how many bytes until the next buffer boundary
            let current_pos = data.len();
            let bytes_until_boundary = buffer_capacity - (current_pos % buffer_capacity);

            // Create line content that will position delimiter to split
            // We want (delimiter_len - 1) bytes before boundary, then 1 byte after
            let line_content = if bytes_until_boundary > delimiter_len {
                let content_len = bytes_until_boundary - (delimiter_len - 1);
                format!("line{i:0width$}", width = content_len.saturating_sub(4)).into_bytes()
            } else {
                // Not enough room in this buffer, pad to next boundary
                let padding = bytes_until_boundary;
                let extra_content = buffer_capacity - (delimiter_len - 1);
                let mut content = vec![b'X'; padding];
                content.extend_from_slice(
                    format!("L{i:0width$}", width = extra_content.saturating_sub(1)).as_bytes(),
                );
                content
            };

            expected_lines.push(line_content.clone());
            data.extend_from_slice(&line_content);
            data.extend_from_slice(delimiter);
        }

        // Now test reading this data
        let cursor = Cursor::new(data);
        let mut reader = BufReader::with_capacity(buffer_capacity, cursor);
        let mut position = 0;
        let max_size = 1024;

        // Read each line and verify it matches expected
        for (i, expected_line) in expected_lines.iter().enumerate() {
            let mut buffer = BytesMut::new();
            let result = read_until_with_max_size(
                &mut reader,
                &mut position,
                delimiter,
                &mut buffer,
                max_size,
            )
            .await
            .unwrap();

            assert_eq!(
                buffer.as_ref(),
                expected_line.as_slice(),
                "Line {i} should match expected content. Got: {:?}, Expected: {:?}",
                String::from_utf8_lossy(&buffer),
                String::from_utf8_lossy(expected_line)
            );

            assert!(
                result.successfully_read.is_some(),
                "Should find delimiter for line {i}"
            );
        }
    }

    #[tokio::test]
    async fn test_single_byte_delimiter_boundary() {
        // Test single-byte delimiter (should work without any special handling)
        test_delimiter_boundary_split_helper(b"\n", 5).await;
    }

    #[tokio::test]
    async fn test_two_byte_delimiter_boundary() {
        // Test two-byte delimiter (CRLF case)
        test_delimiter_boundary_split_helper(b"\r\n", 5).await;
    }

    #[tokio::test]
    async fn test_three_byte_delimiter_boundary() {
        test_delimiter_boundary_split_helper(b"|||", 5).await;
    }

    #[tokio::test]
    async fn test_four_byte_delimiter_boundary() {
        test_delimiter_boundary_split_helper(b"<|>|", 5).await;
    }

    #[tokio::test]
    async fn test_five_byte_delimiter_boundary() {
        test_delimiter_boundary_split_helper(b"<<>>>", 5).await;
    }
}

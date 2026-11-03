use crate::FilePosition;
use std::{cmp::min, io};

use bstr::Finder;
use bytes::BytesMut;
use tokio::io::{AsyncBufRead, AsyncBufReadExt};

pub struct ReadResult {
    pub successfully_read: Option<usize>,
    pub discarded_for_size_and_truncated: Vec<BytesMut>,
}

/// Returns the length of the longest proper prefix of `delim` that `data` ends with.
fn partial_delim_len(data: &[u8], delim: &[u8]) -> usize {
    (1..delim.len())
        .rev()
        .find(|&prefix_len| data.ends_with(&delim[..prefix_len]))
        .unwrap_or(0)
}

/// Read up to `max_size` bytes from `reader`, splitting by `delim`
///
/// The function reads up to `max_size` bytes from `reader`, splitting the input
/// by `delim`. If a `delim` is not found in `reader` before `max_size` bytes
/// are read then the reader is polled until `delim` is found and the results
/// are discarded. Else, the result is written into `buf`.
///
/// The return is unusual. In the Err case this function has not written into
/// `buf` and the caller should not examine its contents. In the Ok case if the
/// inner value is None the caller should retry the call as the buffering read
/// hit the end of the buffer but did not find a `delim` yet, indicating that
/// we've sheered a write or that there were no bytes available in the `reader`
/// and the `reader` was very sure about it. If the inner value is Some the
/// interior `usize` is the number of bytes written into `buf`. When retrying,
/// pass the same `buf` and `buffered_delim_len` back in. A call that stops
/// partway through a `delim` leaves that part at the end of `buf` and sets
/// `buffered_delim_len` to its length, so the next call can still complete it.
///
/// Tweak of
/// <https://github.com/rust-lang/rust/blob/bf843eb9c2d48a80a5992a5d60858e27269f9575/src/libstd/io/mod.rs#L1471>.
///
/// # Performance
///
/// Benchmarks indicate that this function processes in the high single-digit
/// GiB/s range for buffers of length 1KiB. For buffers any smaller than this
/// the overhead of setup dominates our benchmarks.
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
    buffered_delim_len: &'a mut usize,
    max_size: usize,
) -> io::Result<ReadResult> {
    let mut total_read = 0;
    let delim_finder = Finder::new(delim);
    let delim_len = delim.len();
    let mut discarded_for_size_and_truncated = Vec::new();

    // Used to track partial delimiter matches across buffer boundaries.
    // Data is read in chunks from the reader (see `fill_buf` below).
    // A multi-byte delimiter may be split across the "old" and "new" buffers.
    // Any potential partial delimiter that was found in the "old" buffer is stored in this variable.
    let mut partial_delim: BytesMut = BytesMut::with_capacity(delim_len);

    // An earlier call may have reached EOF with part of a delimiter at the end of `buf`. It belongs
    // to neither the line nor its length until the data after it shows what it is. The count comes
    // from that call, because the end of a line being discarded can look the same.
    let line_len = buf.len() - min(std::mem::take(buffered_delim_len), buf.len());
    partial_delim.extend_from_slice(&buf[line_len..]);
    buf.truncate(line_len);

    let mut discarding = false;

    loop {
        // Read the next chunk of data
        let available: &[u8] = match reader.fill_buf().await {
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => {
                buf.extend_from_slice(&partial_delim);
                *buffered_delim_len = partial_delim.len();
                return Err(e);
            }
        };

        // First, check if we have a partial delimiter from the previous iteration/buffer
        let mut delim_suffix_len = None;
        if !partial_delim.is_empty() {
            let expected_suffix = &delim[partial_delim.len()..];

            // We already know that we have a partial delimiter match from the previous buffer.
            // Here we check what part of the delimiter is missing and whether the new buffer
            // contains the remaining part.
            if available.starts_with(expected_suffix) {
                // Complete delimiter found! Consuming the remainder of the delimiter and deciding
                // what to do with the line it ends is the same as for a delimiter found whole.
                delim_suffix_len = Some(expected_suffix.len());
            } else if available.is_empty() {
                // We've hit EOF, so we can't tell yet. Leave the partial delimiter at the end of
                // `buf` for the next call to pick up.
                buf.extend_from_slice(&partial_delim);
                *buffered_delim_len = partial_delim.len();
                return Ok(ReadResult {
                    successfully_read: None,
                    discarded_for_size_and_truncated,
                });
            } else if expected_suffix.starts_with(available) {
                // Still only part of the delimiter, so keep it and wait for the rest.
                partial_delim.extend_from_slice(available);
                let used = available.len();
                reader.consume(used);
                *position += used as u64;
                total_read += used;
                continue;
            } else if !discarding {
                // Not a complete delimiter after all.
                // Add partial_delim to output buffer as it is actual data.
                buf.extend_from_slice(&partial_delim);
            }
            partial_delim.clear();
            // Continue processing current available buffer
        }

        let (done, used) = {
            if let Some(suffix_len) = delim_suffix_len {
                (true, suffix_len)
            } else if let Some(i) = delim_finder.find(available) {
                if !discarding {
                    buf.extend_from_slice(&available[..i]);
                }
                (true, i + delim_len)
            } else {
                // No delimiter found in current buffer. But there could be a partial delimiter
                // at the end of this buffer. For multi-byte delimiters like \r\n, we need
                // to handle the case where the delimiter is split across buffer boundaries
                // (e.g. \r in the "old" buffer, then we read new data and find \n in the new
                // buffer).
                let mut partial_match_len = 0;

                // We only need to check if we're not already at the end of the buffer and if we
                // have a delimiter that has more than one byte.
                if !available.is_empty() && delim_len > 1 {
                    // Check if the end of the current buffer matches a prefix of the delimiter
                    // by testing from longest to shortest possible prefix.
                    //
                    // `partial_delim_len` runs at most (delim_len - 1) iterations:
                    //   - 2-byte delimiter (\r\n): 1 iteration max
                    //   - 5-byte delimiter: 4 iterations max
                    //
                    // This part of the code is only called if all of these are true:
                    //
                    // - We have a new buffer (e.g. every 8kB, i.e. only called once per buffer)
                    // - We have a multi-byte delimiter
                    // - This delimiter could not be found in the current buffer
                    //
                    // Even for longer delimiters the performance impact is negligible.
                    //
                    // Example 1:
                    //   Delimiter: \r\n
                    //   Iteration 1: It checks if the current buffer ends with "\r",
                    //     if it does we have a potential partial delimiter.
                    //   The next chunk will confirm whether this is truly part of a delimiter.

                    // Example 2:
                    //   Delimiter: ABCDE
                    //   Iteration 1: It checks if the current buffer ends with "ABCD" (we don't
                    //     need to check "ABCDE" because that would have been caught by
                    //     `delim_finder.find` earlier)
                    //   Iteration 2: It checks if the current buffer ends with "ABC"
                    //   Iterations 3-4: Same for "AB" and "A"
                    partial_match_len = partial_delim_len(available, delim);
                }

                let bytes_to_copy = available.len() - partial_match_len;

                if !discarding && bytes_to_copy > 0 {
                    buf.extend_from_slice(&available[..bytes_to_copy]);
                }

                // If we found a potential partial delimiter, save it for the next iteration
                if partial_match_len > 0 {
                    partial_delim.clear();
                    partial_delim.extend_from_slice(&available[bytes_to_copy..]);
                }

                (false, available.len())
            }
        };

        // Check if we're at EOF before we start processing
        // (for borrow checker, has to come before `consume`)
        let at_eof = available.is_empty();

        reader.consume(used);
        *position += used as u64; // do this at exactly same time
        total_read += used;

        if !discarding && buf.len() > max_size {
            // keep only the first <1k bytes to make sure we can actually emit a usable error
            let length_to_keep = min(1000, max_size);
            let mut truncated: BytesMut = BytesMut::zeroed(length_to_keep);
            truncated.copy_from_slice(&buf[0..length_to_keep]);
            discarded_for_size_and_truncated.push(truncated);
            discarding = true;
        }

        if done {
            if discarding {
                discarding = false;
                buf.clear();
            } else {
                return Ok(ReadResult {
                    successfully_read: Some(total_read),
                    discarded_for_size_and_truncated,
                });
            }
        } else if used == 0 && at_eof {
            // We've hit EOF but haven't seen a delimiter. This can happen when:
            // 1. The file ends without a trailing delimiter
            // 2. We're observing an incomplete write
            //
            // Return None to signal the caller to retry later.
            return Ok(ReadResult {
                successfully_read: None,
                discarded_for_size_and_truncated,
            });
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        io::{self, Cursor},
        num::NonZeroU8,
        ops::Range,
        pin::Pin,
        task::{Context, Poll},
    };

    use bytes::{BufMut, BytesMut};
    use quickcheck::{QuickCheck, TestResult};
    use tokio::io::{AsyncRead, AsyncReadExt, BufReader, ReadBuf};

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
        let mut buffered_delim_len = 0;
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
                &mut buffered_delim_len,
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
            let mut buffered_delim_len = 0;
            let result = read_until_with_max_size(
                &mut reader,
                &mut position,
                delimiter,
                &mut buffer,
                &mut buffered_delim_len,
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

    /// Reads from `data` once, as a reader that hits EOF right after `data` would.
    async fn read_chunk(
        data: &[u8],
        position: &mut u64,
        delimiter: &[u8],
        buffer: &mut BytesMut,
        buffered_delim_len: &mut usize,
        max_size: usize,
    ) -> ReadResult {
        let mut reader = BufReader::new(Cursor::new(data.to_vec()));
        read_until_with_max_size(
            &mut reader,
            position,
            delimiter,
            buffer,
            buffered_delim_len,
            max_size,
        )
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn test_delimiter_split_by_eof() {
        // With the first limit the merged line used to be dropped, with the second it used to be
        // emitted as one event.
        for max_size in [5, 1024] {
            let delimiter = b"\r\n";
            let mut position = 0;
            let mut buffer = BytesMut::new();
            let mut buffered_delim_len = 0;

            let result = read_chunk(
                b"123\r",
                &mut position,
                delimiter,
                &mut buffer,
                &mut buffered_delim_len,
                max_size,
            )
            .await;
            assert_eq!(result.successfully_read, None);
            assert!(result.discarded_for_size_and_truncated.is_empty());

            // Polling at EOF changes nothing.
            let result = read_chunk(
                b"",
                &mut position,
                delimiter,
                &mut buffer,
                &mut buffered_delim_len,
                max_size,
            )
            .await;
            assert_eq!(result.successfully_read, None);
            assert!(result.discarded_for_size_and_truncated.is_empty());
            assert_eq!(buffer.as_ref(), b"123\r");

            let mut reader = BufReader::new(Cursor::new(b"\nnext\r\n".to_vec()));
            let result = read_until_with_max_size(
                &mut reader,
                &mut position,
                delimiter,
                &mut buffer,
                &mut buffered_delim_len,
                max_size,
            )
            .await
            .unwrap();
            assert_eq!(result.successfully_read, Some(1));
            assert!(result.discarded_for_size_and_truncated.is_empty());
            assert_eq!(buffer.as_ref(), b"123");
            assert_eq!(position, 5);

            buffer.clear();
            let result = read_until_with_max_size(
                &mut reader,
                &mut position,
                delimiter,
                &mut buffer,
                &mut buffered_delim_len,
                max_size,
            )
            .await
            .unwrap();
            assert_eq!(result.successfully_read, Some(6));
            assert!(result.discarded_for_size_and_truncated.is_empty());
            assert_eq!(buffer.as_ref(), b"next");
            assert_eq!(position, 11);
        }
    }

    #[tokio::test]
    async fn test_delimiter_split_by_eof_at_every_position() {
        async fn lines_from_chunks(chunks: [&[u8]; 3], delimiter: &[u8]) -> Vec<BytesMut> {
            let mut lines = Vec::new();
            let mut position = 0;
            let mut buffer = BytesMut::new();
            let mut buffered_delim_len = 0;
            for chunk in chunks {
                let mut reader = BufReader::new(Cursor::new(chunk.to_vec()));
                while read_until_with_max_size(
                    &mut reader,
                    &mut position,
                    delimiter,
                    &mut buffer,
                    &mut buffered_delim_len,
                    1024,
                )
                .await
                .unwrap()
                .successfully_read
                .is_some()
                {
                    lines.push(buffer.split());
                }
            }
            lines
        }

        async fn check(delimiter: &[u8], data: &[u8], expected: &[&[u8]]) {
            for first in 0..=data.len() {
                for second in first..=data.len() {
                    let chunks = [&data[..first], &data[first..second], &data[second..]];
                    assert_eq!(
                        lines_from_chunks(chunks, delimiter).await,
                        expected,
                        "chunks: {chunks:?}"
                    );
                }
            }
        }

        check(
            b"\r\n",
            b"a\r\nb\r\r\nc\rd\r\n\r\n",
            &[b"a", b"b\r", b"c\rd", b""],
        )
        .await;
        check(b"ABC", b"xABCyAABCzABABCABC", &[b"x", b"yA", b"zAB", b""]).await;
    }

    #[tokio::test]
    async fn test_long_delimiter_split_by_eof_twice() {
        let delimiter = b"ABC";
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;

        for chunk in [b"xA".as_slice(), b"B"] {
            let result = read_chunk(
                chunk,
                &mut position,
                delimiter,
                &mut buffer,
                &mut buffered_delim_len,
                1024,
            )
            .await;
            assert_eq!(result.successfully_read, None);
        }

        let result = read_chunk(
            b"C",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            1024,
        )
        .await;
        assert_eq!(result.successfully_read, Some(1));
        assert_eq!(buffer.as_ref(), b"x");
    }

    #[tokio::test]
    async fn test_long_delimiter_split_by_reads_in_one_call() {
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;
        let mut reader = BufReader::with_capacity(1, Cursor::new(b"xABCy".to_vec()));

        let result = read_until_with_max_size(
            &mut reader,
            &mut position,
            b"ABC",
            &mut buffer,
            &mut buffered_delim_len,
            1024,
        )
        .await
        .unwrap();

        assert_eq!(result.successfully_read, Some(4));
        assert_eq!(buffer.as_ref(), b"x");
    }

    #[tokio::test]
    async fn test_partial_delimiter_at_eof_can_turn_out_to_be_data() {
        let delimiter = b"\r\n";
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;

        let result = read_chunk(
            b"12\r",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            1024,
        )
        .await;
        assert_eq!(result.successfully_read, None);

        let result = read_chunk(
            b"x\r\n",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            1024,
        )
        .await;
        assert_eq!(result.successfully_read, Some(3));
        assert_eq!(buffer.as_ref(), b"12\rx");
    }

    #[tokio::test]
    async fn test_partial_delimiter_at_eof_does_not_count_towards_the_limit() {
        let delimiter = b"\r\n";
        let max_size = 5;
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;

        // The line is exactly at the limit, only its partial delimiter would take it over.
        let result = read_chunk(
            b"12345\r",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            max_size,
        )
        .await;
        assert_eq!(result.successfully_read, None);
        assert!(result.discarded_for_size_and_truncated.is_empty());

        let result = read_chunk(
            b"\n",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            max_size,
        )
        .await;
        assert_eq!(result.successfully_read, Some(1));
        assert!(result.discarded_for_size_and_truncated.is_empty());
        assert_eq!(buffer.as_ref(), b"12345");
    }

    #[tokio::test]
    async fn test_oversized_line_with_delimiter_split_by_eof() {
        let delimiter = b"\r\n";
        let max_size = 5;
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;

        let result = read_chunk(
            b"123456\r",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            max_size,
        )
        .await;
        assert_eq!(result.successfully_read, None);
        assert_eq!(result.discarded_for_size_and_truncated.len(), 1);

        // The oversized line ends here and is not emitted, and the next line survives.
        let result = read_chunk(
            b"\nnext\r\n",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            max_size,
        )
        .await;
        assert_eq!(result.successfully_read, Some(7));
        assert_eq!(buffer.as_ref(), b"next");
        assert_eq!(position, 14);
    }

    #[tokio::test]
    async fn test_oversized_line_with_delimiter_split_by_buffer_boundary() {
        let delimiter = b"\r\n";
        let max_size = 5;
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;
        // The first fill of the buffer ends between the `\r` and the `\n`.
        let mut reader = BufReader::with_capacity(7, Cursor::new(b"123456\r\nnext\r\n".to_vec()));

        let result = read_until_with_max_size(
            &mut reader,
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            max_size,
        )
        .await
        .unwrap();

        assert_eq!(result.successfully_read, Some(14));
        assert_eq!(result.discarded_for_size_and_truncated.len(), 1);
        assert_eq!(buffer.as_ref(), b"next");
    }

    #[tokio::test]
    async fn test_read_error_keeps_partial_delimiter() {
        struct FailingReader;

        impl AsyncRead for FailingReader {
            fn poll_read(
                self: Pin<&mut Self>,
                _: &mut Context<'_>,
                _: &mut ReadBuf<'_>,
            ) -> Poll<io::Result<()>> {
                Poll::Ready(Err(io::Error::other("read failed")))
            }
        }

        let delimiter = b"\r\n";
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;

        // The error comes after the partial delimiter has been read in the same call.
        let mut reader = BufReader::new(Cursor::new(b"12\r".to_vec()).chain(FailingReader));
        let result = read_until_with_max_size(
            &mut reader,
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            1024,
        )
        .await;
        assert!(result.is_err());

        // The error comes when an earlier call already left the partial delimiter in `buffer`.
        let mut reader = BufReader::new(FailingReader);
        let result = read_until_with_max_size(
            &mut reader,
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            1024,
        )
        .await;
        assert!(result.is_err());

        let result = read_chunk(
            b"\n",
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            1024,
        )
        .await;
        assert_eq!(result.successfully_read, Some(1));
        assert_eq!(buffer.as_ref(), b"12");
    }

    #[tokio::test]
    async fn test_discarded_bytes_are_not_taken_for_a_partial_delimiter() {
        let delimiter = b"\r\n";
        let max_size = 5;
        let mut position = 0;
        let mut buffer = BytesMut::new();
        let mut buffered_delim_len = 0;

        // The line crosses the limit with `\r` as its sixth byte, so the discarded part kept in
        // `buffer` ends with `\r`, while the file ends with `x`.
        let mut reader = BufReader::with_capacity(7, Cursor::new(b"12345\r\rx".to_vec()));
        let result = read_until_with_max_size(
            &mut reader,
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            max_size,
        )
        .await
        .unwrap();
        assert_eq!(result.successfully_read, None);

        // There is no delimiter before the final one, so this is all one oversized line.
        let mut reader = BufReader::new(Cursor::new(b"\nnext\r\n".to_vec()));
        let result = read_until_with_max_size(
            &mut reader,
            &mut position,
            delimiter,
            &mut buffer,
            &mut buffered_delim_len,
            max_size,
        )
        .await
        .unwrap();
        assert_eq!(result.successfully_read, None);
        assert!(buffer.is_empty());
        assert_eq!(position, 15);
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

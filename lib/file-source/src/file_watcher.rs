use crate::{FilePosition, ReadFrom};
use bstr::Finder;
use bytes::{Bytes, BytesMut};
use chrono::{DateTime, Utc};
use flate2::bufread::MultiGzDecoder;
use std::{
    fs::{self, File},
    io::{self, BufRead, Seek},
    path::PathBuf,
    time::{Duration, Instant},
};

use crate::metadata_ext::PortableFileExt;

/// The `FileWatcher` struct defines the polling based state machine which reads
/// from a file path, transparently updating the underlying file descriptor when
/// the file has been rolled over, as is common for logs.
///
/// The `FileWatcher` is expected to live for the lifetime of the file
/// path. `FileServer` is responsible for clearing away `FileWatchers` which no
/// longer exist.
pub struct FileWatcher {
    pub path: PathBuf,
    findable: bool,
    reader: Box<dyn BufRead>,
    file_position: FilePosition,
    devno: u64,
    inode: u64,
    is_dead: bool,
    last_read_attempt: Instant,
    last_read_success: Instant,
    max_line_bytes: usize,
    line_delimiter: Bytes,
    buf: BytesMut,
}

impl FileWatcher {
    /// Create a new `FileWatcher`
    ///
    /// The input path will be used by `FileWatcher` to prime its state
    /// machine. A `FileWatcher` tracks _only one_ file. This function returns
    /// None if the path does not exist or is not readable by the current process.
    pub fn new(
        path: PathBuf,
        read_from: ReadFrom,
        ignore_before: Option<DateTime<Utc>>,
        max_line_bytes: usize,
        line_delimiter: Bytes,
    ) -> Result<FileWatcher, io::Error> {
        let f = fs::File::open(&path)?;
        let (devno, ino) = (f.portable_dev()?, f.portable_ino()?);
        let metadata = f.metadata()?;
        let mut reader = io::BufReader::new(f);

        let too_old = if let (Some(ignore_before), Ok(modified_time)) = (
            ignore_before,
            metadata.modified().map(DateTime::<Utc>::from),
        ) {
            modified_time < ignore_before
        } else {
            false
        };

        let gzipped = is_gzipped(&mut reader)?;

        // Determine the actual position at which we should start reading
        let (reader, file_position): (Box<dyn BufRead>, FilePosition) =
            match (gzipped, too_old, read_from) {
                (true, true, _) => {
                    debug!(
                        message = "Not reading gzipped file older than `ignore_older`.",
                        ?path,
                    );
                    (Box::new(null_reader()), 0)
                }
                (true, _, ReadFrom::Checkpoint(file_position)) => {
                    debug!(
                        message = "Not re-reading gzipped file with existing stored offset.",
                        ?path,
                        %file_position
                    );
                    (Box::new(null_reader()), file_position)
                }
                // TODO: This may become the default, leading us to stop reading gzipped files that
                // we were reading before. Should we merge this and the next branch to read
                // compressed file from the beginning even when `read_from = "end"` (implicitly via
                // default or explicitly via config)?
                (true, _, ReadFrom::End) => {
                    debug!(
                        message = "Can't read from the end of already-compressed file.",
                        ?path,
                    );
                    (Box::new(null_reader()), 0)
                }
                (true, false, ReadFrom::Beginning) => {
                    (Box::new(io::BufReader::new(MultiGzDecoder::new(reader))), 0)
                }
                (false, true, _) => {
                    let pos = reader.seek(io::SeekFrom::End(0)).unwrap();
                    (Box::new(reader), pos)
                }
                (false, false, ReadFrom::Checkpoint(file_position)) => {
                    let pos = reader.seek(io::SeekFrom::Start(file_position)).unwrap();
                    (Box::new(reader), pos)
                }
                (false, false, ReadFrom::Beginning) => {
                    let pos = reader.seek(io::SeekFrom::Start(0)).unwrap();
                    (Box::new(reader), pos)
                }
                (false, false, ReadFrom::End) => {
                    let pos = reader.seek(io::SeekFrom::End(0)).unwrap();
                    (Box::new(reader), pos)
                }
            };

        let ts = metadata
            .modified()
            .ok()
            .and_then(|mtime| mtime.elapsed().ok())
            .and_then(|diff| Instant::now().checked_sub(diff))
            .unwrap_or_else(Instant::now);

        Ok(FileWatcher {
            path,
            findable: true,
            reader,
            file_position,
            devno,
            inode: ino,
            is_dead: false,
            last_read_attempt: ts,
            last_read_success: ts,
            max_line_bytes,
            line_delimiter,
            buf: BytesMut::new(),
        })
    }

    pub fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let file_handle = File::open(&path)?;
        if (file_handle.portable_dev()?, file_handle.portable_ino()?) != (self.devno, self.inode) {
            let mut reader = io::BufReader::new(fs::File::open(&path)?);
            let gzipped = is_gzipped(&mut reader)?;
            let new_reader: Box<dyn BufRead> = if gzipped {
                if self.file_position != 0 {
                    Box::new(null_reader())
                } else {
                    Box::new(io::BufReader::new(MultiGzDecoder::new(reader)))
                }
            } else {
                reader.seek(io::SeekFrom::Start(self.file_position))?;
                Box::new(reader)
            };
            self.reader = new_reader;
            self.devno = file_handle.portable_dev()?;
            self.inode = file_handle.portable_ino()?;
        }
        self.path = path;
        Ok(())
    }

    pub fn set_file_findable(&mut self, f: bool) {
        self.findable = f;
    }

    pub fn file_findable(&self) -> bool {
        self.findable
    }

    pub fn set_dead(&mut self) {
        self.is_dead = true;
    }

    pub fn dead(&self) -> bool {
        self.is_dead
    }

    pub fn get_file_position(&self) -> FilePosition {
        self.file_position
    }

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time. `read_line` will open
    /// a new file handler as needed, transparently to the caller.
    pub fn read_line(&mut self) -> io::Result<Option<Bytes>> {
        self.track_read_attempt();

        let reader = &mut self.reader;
        let file_position = &mut self.file_position;
        match read_until_with_max_size(
            reader,
            file_position,
            self.line_delimiter.as_ref(),
            &mut self.buf,
            self.max_line_bytes,
        ) {
            Ok(Some(_)) => {
                self.track_read_success();
                Ok(Some(self.buf.split().freeze()))
            }
            Ok(None) => {
                if !self.file_findable() {
                    self.set_dead();
                    // File has been deleted, so return what we have in the buffer, even though it
                    // didn't end with a newline. This is not a perfect signal for when we should
                    // give up waiting for a newline, but it's decent.
                    Ok(Some(self.buf.split().freeze()))
                } else {
                    Ok(None)
                }
            }
            Err(e) => {
                if let io::ErrorKind::NotFound = e.kind() {
                    self.set_dead();
                }
                Err(e)
            }
        }
    }

    #[inline]
    fn track_read_attempt(&mut self) {
        self.last_read_attempt = Instant::now();
    }

    #[inline]
    fn track_read_success(&mut self) {
        self.last_read_success = Instant::now();
    }

    #[inline]
    pub fn last_read_success(&self) -> Instant {
        self.last_read_success
    }

    #[inline]
    pub fn should_read(&self) -> bool {
        self.last_read_success.elapsed() < Duration::from_secs(10)
            || self.last_read_attempt.elapsed() > Duration::from_secs(10)
    }
}

fn is_gzipped(r: &mut io::BufReader<fs::File>) -> io::Result<bool> {
    let header_bytes = r.fill_buf()?;
    Ok(header_bytes.starts_with(&[0x1f, 0x8b]))
}

fn null_reader() -> impl BufRead {
    io::Cursor::new(Vec::new())
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
/// interior `usize` is the number of bytes written into `buf`.
///
/// Tweak of
/// https://github.com/rust-lang/rust/blob/bf843eb9c2d48a80a5992a5d60858e27269f9575/src/libstd/io/mod.rs#L1471
fn read_until_with_max_size<R: BufRead + ?Sized>(
    reader: &mut R,
    position: &mut FilePosition,
    delim: &[u8],
    buf: &mut BytesMut,
    max_size: usize,
) -> io::Result<Option<usize>> {
    let mut total_read = 0;
    let mut discarding = false;
    let delim_finder = Finder::new(delim);
    let delim_len = delim.len();
    loop {
        let available: &[u8] = match reader.fill_buf() {
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        let (done, used) = {
            match delim_finder.find(available) {
                Some(i) => {
                    if !discarding {
                        buf.extend_from_slice(&available[..i]);
                    }
                    (true, i + delim_len)
                }
                None => {
                    if !discarding {
                        buf.extend_from_slice(available);
                    }
                    (false, available.len())
                }
            }
        };
        reader.consume(used);
        *position += used as u64; // do this at exactly same time
        total_read += used;

        if !discarding && buf.len() > max_size {
            warn!(
                message = "Found line that exceeds max_line_bytes; discarding.",
                internal_log_rate_secs = 30
            );
            discarding = true;
        }

        if done {
            if !discarding {
                return Ok(Some(total_read));
            } else {
                discarding = false;
                buf.clear();
            }
        } else if used == 0 {
            // We've hit EOF but not yet seen a newline. This can happen when unlucky timing causes
            // us to observe an incomplete write. We return None here and let the loop continue
            // next time the method is called. This is safe because the buffer is specific to this
            // FileWatcher.
            return Ok(None);
        }
    }
}

#[cfg(test)]
mod test {
    use super::read_until_with_max_size;
    use bytes::{BufMut, BytesMut};
    use quickcheck::{QuickCheck, TestResult};
    use std::io::Cursor;
    use std::num::NonZeroU8;
    use std::ops::Range;

    fn qc_inner(chunks: Vec<Vec<u8>>, delim: u8, max_size: NonZeroU8) -> TestResult {
        // The `global_data` is the view of `chunks` as a single contiguous
        // block of memory. Where `chunks` simulates what happens when bytes are
        // fitfully available `global_data` is the view of all chunks assembled
        // after every byte is available.
        let mut global_data = BytesMut::new();

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
        // chunk stream. As such, discover which that it and record it
        // specially.
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
            let mut reader = Cursor::new(&chunk);

            match read_until_with_max_size(
                &mut reader,
                &mut position,
                &delimiter,
                &mut buffer,
                max_size.get() as usize,
            )
            .unwrap()
            {
                None => {
                    // Subject only returns None if this is the last chunk _and_
                    // the chunk did not contain a delimiter _or_ the delimiter
                    // was outside the max_size range _or_ the current chunk is empty.
                    let has_valid_delimiter = facts
                        .iter()
                        .any(|details| ((details.chunk_index == idx) && details.within_max_size));
                    assert!(chunk.is_empty() || !has_valid_delimiter)
                }
                Some(total_read) => {
                    // Now that the function has returned we confirm that the
                    // returned details match our `first_delim` and also that
                    // the `buffer` is populated correctly.
                    assert!(first_delim.is_some());
                    assert_eq!(
                        first_delim.clone().unwrap().global_index + 1,
                        position as usize
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

    #[test]
    fn qc_read_until_with_max_size() {
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
        QuickCheck::new()
            .tests(1_000)
            .max_tests(2_000)
            .quickcheck(qc_inner as fn(Vec<Vec<u8>>, u8, NonZeroU8) -> TestResult);
    }
}

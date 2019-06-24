use std::fs;
use std::io::{self, BufRead, Seek};
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::time;

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
    reader: Option<io::BufReader<fs::File>>,
    previous_size: u64,
    devno: u64,
    inode: u64,
}

impl FileWatcher {
    /// Create a new `FileWatcher`
    ///
    /// The input path will be used by `FileWatcher` to prime its state
    /// machine. A `FileWatcher` tracks _only one_ file. This function returns
    /// None if the path does not exist or is not readable by cernan.
    pub fn new(
        path: PathBuf,
        start_at_beginning: bool,
        ignore_before: Option<time::SystemTime>,
    ) -> io::Result<FileWatcher> {
        match fs::File::open(&path) {
            Ok(f) => {
                let metadata = f.metadata()?;
                let mut rdr = io::BufReader::new(f);

                let too_old = if let (Some(ignore_before), Ok(mtime)) =
                    (ignore_before, metadata.modified())
                {
                    mtime < ignore_before
                } else {
                    false
                };

                if !start_at_beginning || too_old {
                    assert!(rdr.seek(io::SeekFrom::End(0)).is_ok());
                }

                Ok(FileWatcher {
                    path: path,
                    findable: true,
                    reader: Some(rdr),
                    previous_size: 0,
                    devno: metadata.dev(),
                    inode: metadata.ino(),
                })
            }
            Err(e) => match e.kind() {
                io::ErrorKind::NotFound => {
                    let fw = {
                        FileWatcher {
                            path: path,
                            findable: true,
                            reader: None,
                            previous_size: 0,
                            devno: 0,
                            inode: 0,
                        }
                    };
                    Ok(fw)
                }
                _ => Err(e),
            },
        }
    }

    pub fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        assert!(self.reader.is_some());
        let metadata = fs::metadata(&path)?;
        let (devno, inode) = (metadata.dev(), metadata.ino());
        if (devno, inode) != (self.devno, self.inode) {
            let old_reader = self.reader.as_mut().unwrap();
            let position = old_reader.seek(io::SeekFrom::Current(0))?;
            let f = fs::File::open(&path)?;
            let mut new_reader = io::BufReader::new(f);
            new_reader.seek(io::SeekFrom::Start(position))?;
            self.reader = Some(new_reader);
            self.devno = devno;
            self.inode = inode;
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
        self.reader = None;
    }

    pub fn dead(&self) -> bool {
        self.reader.is_none()
    }

    /// Read a single line from the underlying file
    ///
    /// This function will attempt to read a new line from its file, blocking,
    /// up to some maximum but unspecified amount of time. `read_line` will open
    /// a new file handler at need, transparently to the caller.
    pub fn read_line(&mut self, mut buffer: &mut Vec<u8>, max_size: usize) -> io::Result<usize> {
        //ensure buffer is re-initialized
        buffer.clear();
        if let Some(ref mut reader) = self.reader {
            // Every read we detect the current_size of the file and compare
            // against the previous_size. There are three cases to consider:
            //
            //  * current_size > previous_size
            //  * current_size == previous_size
            //  * current_size < previous_size
            //
            // In the last case we must consider that the file has been
            // truncated and we can no longer trust our seek position
            // in-file. We MUST seek back to position 0. This is the _simplest_
            // case to handle.
            //
            // Consider the equality case. It's possible that NO WRITES have
            // come into the file _or_ that the file has been truncated and
            // coincidentally the new writes exactly match the byte size of the
            // previous writes. THESE WRITES WILL BE LOST.
            //
            // Now the greater than inequality. All of the equality
            // considerations hold for this case. Also, consider if a write
            // straddles the line between previous_size and current_size. Then
            // we will be UNABLE to determine the proper start index of this
            // write and we WILL return a partial write of length
            // absolute_write_idx - previous_size.
            let current_size = reader.get_ref().metadata().unwrap().size();
            if self.previous_size <= current_size {
                self.previous_size = current_size;
                // match here on error, if metadata doesn't match up open_at_start
                // new reader and let it catch on the next looparound
                match read_until_with_max_size(reader, b'\n', &mut buffer, max_size) {
                    Ok(0) => {
                        if !self.file_findable() {
                            self.set_dead();
                        }
                        Ok(0)
                    }
                    Ok(sz) => Ok(sz),
                    Err(e) => {
                        if let io::ErrorKind::NotFound = e.kind() {
                            self.set_dead();
                        }
                        Err(e)
                    }
                }
            } else {
                self.set_dead();
                Ok(0)
            }
        } else {
            Ok(0)
        }
    }
}

// Tweak of https://github.com/rust-lang/rust/blob/bf843eb9c2d48a80a5992a5d60858e27269f9575/src/libstd/io/mod.rs#L1471
// After more than max_size bytes are read as part of a single line, this discard the remaining bytes
// in that line, and then starts again on the next line.
fn read_until_with_max_size<R: BufRead + ?Sized>(
    r: &mut R,
    delim: u8,
    buf: &mut Vec<u8>,
    max_size: usize,
) -> io::Result<usize> {
    let mut total_read = 0;
    let mut discarding = false;
    loop {
        let available = match r.fill_buf() {
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        let (done, used) = {
            // TODO: use memchr to make this faster
            match available.iter().position(|&b| b == delim) {
                Some(i) => {
                    if !discarding {
                        buf.extend_from_slice(&available[..i]);
                    }
                    (true, i + 1)
                }
                None => {
                    if !discarding {
                        buf.extend_from_slice(available);
                    }
                    (false, available.len())
                }
            }
        };
        r.consume(used);
        total_read += used;

        if !discarding && buf.len() > max_size {
            warn!("Found line that exceeds max_line_bytes; discarding.");
            discarding = true;
        }

        if done && discarding {
            discarding = false;
            buf.clear();
        } else if done || used == 0 {
            return Ok(total_read);
        }
    }
}

#[cfg(test)]
mod test {
    use super::read_until_with_max_size;
    use std::io::Cursor;

    #[test]
    fn test_read_until_with_max_size() {
        let mut buf = Cursor::new(&b"12"[..]);
        let mut v = Vec::new();
        assert_eq!(
            read_until_with_max_size(&mut buf, b'3', &mut v, 1000).unwrap(),
            2
        );
        assert_eq!(v, b"12");

        let mut buf = Cursor::new(&b"1233"[..]);
        let mut v = Vec::new();
        assert_eq!(
            read_until_with_max_size(&mut buf, b'3', &mut v, 1000).unwrap(),
            3
        );
        assert_eq!(v, b"12");
        v.truncate(0);
        assert_eq!(
            read_until_with_max_size(&mut buf, b'3', &mut v, 1000).unwrap(),
            1
        );
        assert_eq!(v, b"");
        v.truncate(0);
        assert_eq!(
            read_until_with_max_size(&mut buf, b'3', &mut v, 1000).unwrap(),
            0
        );
        assert_eq!(v, []);

        let mut buf = Cursor::new(&b"short\nthis is too long\nexact size\n11 eleven11\n"[..]);
        let mut v = Vec::new();
        assert_eq!(
            read_until_with_max_size(&mut buf, b'\n', &mut v, 10).unwrap(),
            6
        );
        assert_eq!(v, b"short");
        v.truncate(0);
        assert_eq!(
            read_until_with_max_size(&mut buf, b'\n', &mut v, 10).unwrap(),
            28
        );
        assert_eq!(v, b"exact size");
        v.truncate(0);
        assert_eq!(
            read_until_with_max_size(&mut buf, b'\n', &mut v, 10).unwrap(),
            12
        );
        assert_eq!(v, []);
    }
}

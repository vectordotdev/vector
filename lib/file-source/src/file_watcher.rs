use crate::FilePosition;
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
    reader: io::BufReader<fs::File>,
    file_position: FilePosition,
    devno: u64,
    inode: u64,
    is_dead: bool,
}

impl FileWatcher {
    /// Create a new `FileWatcher`
    ///
    /// The input path will be used by `FileWatcher` to prime its state
    /// machine. A `FileWatcher` tracks _only one_ file. This function returns
    /// None if the path does not exist or is not readable by the current process.
    pub fn new(
        path: PathBuf,
        file_position: FilePosition,
        ignore_before: Option<time::SystemTime>,
    ) -> Result<FileWatcher, io::Error> {
        let f = fs::File::open(&path)?;
        let metadata = f.metadata()?;
        let mut rdr = io::BufReader::new(f);

        let too_old = if let (Some(ignore_before), Ok(modified_time)) =
            (ignore_before, metadata.modified())
        {
            modified_time < ignore_before
        } else {
            false
        };

        let file_position = if too_old {
            rdr.seek(io::SeekFrom::End(0)).unwrap()
        } else {
            rdr.seek(io::SeekFrom::Start(file_position)).unwrap()
        };

        Ok(FileWatcher {
            path: path,
            findable: true,
            reader: rdr,
            file_position: file_position,
            devno: metadata.dev(),
            inode: metadata.ino(),
            is_dead: false,
        })
    }

    pub fn update_path(&mut self, path: PathBuf) -> io::Result<()> {
        let metadata = fs::metadata(&path)?;
        if (metadata.dev(), metadata.ino()) != (self.devno, self.inode) {
            let mut new_reader = io::BufReader::new(fs::File::open(&path)?);
            new_reader.seek(io::SeekFrom::Start(self.file_position))?;
            self.reader = new_reader;
            self.devno = metadata.dev();
            self.inode = metadata.ino();
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
    /// a new file handler at need, transparently to the caller.
    pub fn read_line(&mut self, mut buffer: &mut Vec<u8>, max_size: usize) -> io::Result<usize> {
        //ensure buffer is re-initialized
        buffer.clear();
        let reader = &mut self.reader;
        let file_position = &mut self.file_position;
        match read_until_with_max_size(reader, file_position, b'\n', &mut buffer, max_size) {
            Ok(sz) => {
                if sz == 0 && !self.file_findable() {
                    self.set_dead();
                }
                Ok(sz)
            }
            Err(e) => {
                if let io::ErrorKind::NotFound = e.kind() {
                    self.set_dead();
                }
                Err(e)
            }
        }
    }
}

// Tweak of https://github.com/rust-lang/rust/blob/bf843eb9c2d48a80a5992a5d60858e27269f9575/src/libstd/io/mod.rs#L1471
// After more than max_size bytes are read as part of a single line, this discard the remaining bytes
// in that line, and then starts again on the next line.
fn read_until_with_max_size<R: BufRead + ?Sized>(
    r: &mut R,
    p: &mut FilePosition,
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
        *p += used as u64; // do this at exactly same time
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
        let mut pos = 0;
        let mut v = Vec::new();
        let p = read_until_with_max_size(&mut buf, &mut pos, b'3', &mut v, 1000).unwrap();
        assert_eq!(pos, 2);
        assert_eq!(p, 2);
        assert_eq!(v, b"12");

        let mut buf = Cursor::new(&b"1233"[..]);
        let mut pos = 0;
        let mut v = Vec::new();
        let p = read_until_with_max_size(&mut buf, &mut pos, b'3', &mut v, 1000).unwrap();
        assert_eq!(pos, 3);
        assert_eq!(p, 3);
        assert_eq!(v, b"12");
        v.truncate(0);
        let p = read_until_with_max_size(&mut buf, &mut pos, b'3', &mut v, 1000).unwrap();
        assert_eq!(pos, 4);
        assert_eq!(p, 1);
        assert_eq!(v, b"");
        v.truncate(0);
        let p = read_until_with_max_size(&mut buf, &mut pos, b'3', &mut v, 1000).unwrap();
        assert_eq!(pos, 4);
        assert_eq!(p, 0);
        assert_eq!(v, []);

        let mut buf = Cursor::new(&b"short\nthis is too long\nexact size\n11 eleven11\n"[..]);
        let mut pos = 0;
        let mut v = Vec::new();
        let p = read_until_with_max_size(&mut buf, &mut pos, b'\n', &mut v, 10).unwrap();
        assert_eq!(pos, 6);
        assert_eq!(p, 6);
        assert_eq!(v, b"short");
        v.truncate(0);
        let p = read_until_with_max_size(&mut buf, &mut pos, b'\n', &mut v, 10).unwrap();
        assert_eq!(pos, 34);
        assert_eq!(p, 28);
        assert_eq!(v, b"exact size");
        v.truncate(0);
        let p = read_until_with_max_size(&mut buf, &mut pos, b'\n', &mut v, 10).unwrap();
        assert_eq!(pos, 46);
        assert_eq!(p, 12);
        assert_eq!(v, []);
    }
}

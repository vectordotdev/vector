use crate::{file_watcher::FileWatcher, FileFingerprint, FilePosition};
use bytes::Bytes;
use futures::{stream, Future, Sink, Stream};
use glob::{glob, Pattern};
use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Seek};
use std::path::{Path, PathBuf};
use std::sync::mpsc::RecvTimeoutError;
use std::time;
use tracing::field;

/// `FileServer` is a Source which cooperatively schedules reads over files,
/// converting the lines of said files into `LogLine` structures. As
/// `FileServer` is intended to be useful across multiple operating systems with
/// POSIX filesystem semantics `FileServer` must poll for changes. That is, no
/// event notification is used by `FileServer`.
///
/// `FileServer` is configured on a path to watch. The files do _not_ need to
/// exist at startup. `FileServer` will discover new files which match
/// its path in at most 60 seconds.
pub struct FileServer {
    pub include: Vec<PathBuf>,
    pub exclude: Vec<PathBuf>,
    pub max_read_bytes: usize,
    pub start_at_beginning: bool,
    pub ignore_before: Option<time::SystemTime>,
    pub max_line_bytes: usize,
    pub fingerprint_bytes: usize,
    pub ignored_header_bytes: usize,
    pub data_dir: PathBuf,
    pub glob_minimum_cooldown: time::Duration,
}

/// `FileServer` as Source
///
/// The 'run' of `FileServer` performs the cooperative scheduling of reads over
/// `FileServer`'s configured files. Much care has been taking to make this
/// scheduling 'fair', meaning busy files do not drown out quiet files or vice
/// versa but there's no one perfect approach. Very fast files _will_ be lost if
/// your system aggressively rolls log files. `FileServer` will keep a file
/// handler open but should your system move so quickly that a file disappears
/// before `FileServer` is able to open it the contents will be lost. This should be a
/// rare occurence.
///
/// Specific operating systems support evented interfaces that correct this
/// problem but your intrepid authors know of no generic solution.
impl FileServer {
    pub fn run(
        self,
        mut chans: impl Sink<SinkItem = (Bytes, String), SinkError = ()>,
        shutdown: std::sync::mpsc::Receiver<()>,
    ) {
        let mut read_from_beginning = self.start_at_beginning;
        let mut line_buffer = Vec::new();
        let mut fingerprint_buffer = Vec::new();

        let mut fp_map: HashMap<FileFingerprint, FileWatcher> = Default::default();

        let mut backoff_cap: usize = 1;
        let mut lines = Vec::new();

        let mut checkpointer = Checkpointer::new(&self.data_dir);
        checkpointer.read_checkpoints(self.ignore_before);

        // Alright friends, how does this work?
        //
        // We want to avoid burning up users' CPUs. To do this we sleep after
        // reading lines out of files. But! We want to be responsive as well. We
        // keep track of a 'backoff_cap' to decide how long we'll wait in any
        // given loop. This cap grows each time we fail to read lines in an
        // exponential fashion to some hard-coded cap. To reduce time using glob,
        // we do not re-scan for major file changes (new files, moves, deletes),
        // or write new checkpoints, on every iteration.
        let mut next_glob_time = time::Instant::now();
        loop {
            // Glob find files to follow, but not too often.
            let now_time = time::Instant::now();
            if next_glob_time <= now_time {
                // Schedule the next glob time.
                next_glob_time = now_time.checked_add(self.glob_minimum_cooldown).unwrap();

                // Write any stored checkpoints (uses glob to find old checkpoints).
                checkpointer
                    .write_checkpoints()
                    .map_err(|e| warn!("Problem writing checkpoints: {:?}", e))
                    .ok();

                // Search (glob) for files to detect major file changes.
                let exclude_patterns = self
                    .exclude
                    .iter()
                    .map(|e| Pattern::new(e.to_str().expect("no ability to glob")).unwrap())
                    .collect::<Vec<_>>();
                for (_file_id, watcher) in &mut fp_map {
                    watcher.set_file_findable(false); // assume not findable until found
                }
                for include_pattern in &self.include {
                    for path in glob(include_pattern.to_str().expect("no ability to glob"))
                        .expect("Failed to read glob pattern")
                        .filter_map(Result::ok)
                    {
                        if exclude_patterns
                            .iter()
                            .any(|e| e.matches(path.to_str().unwrap()))
                        {
                            continue;
                        }

                        if let Ok(file_id) =
                            self.get_fingerprint_of_file(&path, &mut fingerprint_buffer)
                        {
                            if let Some(watcher) = fp_map.get_mut(&file_id) {
                                // file fingerprint matches a watched file
                                let was_found_this_cycle = watcher.file_findable();
                                watcher.set_file_findable(true);
                                if watcher.path == path {
                                    trace!(
                                        message = "Continue watching file.",
                                        path = field::debug(&path),
                                    );
                                } else {
                                    // matches a file with a different path
                                    if !was_found_this_cycle {
                                        info!(
                                            message = "Watched file has been renamed.",
                                            path = field::debug(&path),
                                            old_path = field::debug(&watcher.path)
                                        );
                                        watcher.update_path(path).ok(); // ok if this fails: might fix next cycle
                                    } else {
                                        info!(
                                            message = "More than one file has same fingerprint.",
                                            path = field::debug(&path),
                                            old_path = field::debug(&watcher.path)
                                        );
                                        let (old_path, new_path) = (&watcher.path, &path);
                                        if let (Ok(old_modified_time), Ok(new_modified_time)) = (
                                            fs::metadata(&old_path).and_then(|m| m.modified()),
                                            fs::metadata(&new_path).and_then(|m| m.modified()),
                                        ) {
                                            if old_modified_time < new_modified_time {
                                                info!(
                                                        message = "Switching to watch most recently modified file.",
                                                        new_modified_time = field::debug(&new_modified_time),
                                                        old_modified_time = field::debug(&old_modified_time),
                                                        );
                                                watcher.update_path(path).ok(); // ok if this fails: might fix next cycle
                                            }
                                        }
                                    }
                                }
                            } else {
                                // untracked file fingerprint
                                let file_position = if read_from_beginning {
                                    0
                                } else {
                                    checkpointer.get_checkpoint(file_id).unwrap_or(0)
                                };
                                if let Ok(mut watcher) =
                                    FileWatcher::new(path, file_position, self.ignore_before)
                                {
                                    info!(
                                        message = "Found file to watch.",
                                        path = field::debug(&watcher.path),
                                        file_position = field::debug(&file_position),
                                    );
                                    watcher.set_file_findable(true);
                                    fp_map.insert(file_id, watcher);
                                };
                            }
                        }
                    }
                }
                // This special flag only applies to first iteration on startup.
                read_from_beginning = false;
            }

            // Collect lines by polling files.
            let mut global_bytes_read: usize = 0;
            for (&file_id, watcher) in &mut fp_map {
                let mut bytes_read: usize = 0;
                while let Ok(sz) = watcher.read_line(&mut line_buffer, self.max_line_bytes) {
                    if sz > 0 {
                        trace!(
                            message = "Read bytes.",
                            path = field::debug(&watcher.path),
                            bytes = field::debug(sz)
                        );

                        bytes_read += sz;

                        if !line_buffer.is_empty() {
                            lines.push((
                                line_buffer.clone().into(),
                                watcher.path.to_str().expect("not a valid path").to_owned(),
                            ));
                            line_buffer.clear();
                        }
                    } else {
                        break;
                    }
                    if bytes_read > self.max_read_bytes {
                        break;
                    }
                }
                if bytes_read > 0 {
                    global_bytes_read = global_bytes_read.saturating_add(bytes_read);
                    checkpointer.set_checkpoint(file_id, watcher.get_file_position());
                }
            }

            // A FileWatcher is dead when the underlying file has disappeared.
            // If the FileWatcher is dead we don't retain it; it will be deallocated.
            fp_map.retain(|_file_id, watcher| !watcher.dead());

            match stream::iter_ok::<_, ()>(lines.drain(..))
                .forward(chans)
                .wait()
            {
                Ok((_, sink)) => chans = sink,
                Err(_) => {
                    debug!("Output channel closed.");
                    return;
                }
            }
            // When no lines have been read we kick the backup_cap up by twice,
            // limited by the hard-coded cap. Else, we set the backup_cap to its
            // minimum on the assumption that next time through there will be
            // more lines to read promptly.
            if global_bytes_read == 0 {
                let lim = backoff_cap.saturating_mul(2);
                if lim > 2_048 {
                    backoff_cap = 2_048;
                } else {
                    backoff_cap = lim;
                }
            } else {
                backoff_cap = 1;
            }
            let backoff = backoff_cap.saturating_sub(global_bytes_read);

            match shutdown.recv_timeout(time::Duration::from_millis(backoff as u64)) {
                Ok(()) => unreachable!(), // The sender should never actually send
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }

    fn get_fingerprint_of_file(
        &self,
        path: &PathBuf,
        buffer: &mut Vec<u8>,
    ) -> Result<FileFingerprint, io::Error> {
        let i = self.ignored_header_bytes as u64;
        let b = self.fingerprint_bytes;
        buffer.resize(b, 0u8);
        let mut fp = fs::File::open(path)?;
        fp.seek(io::SeekFrom::Start(i))?;
        fp.read_exact(&mut buffer[..b])?;
        let fingerprint = crc::crc64::checksum_ecma(&buffer[..b]);
        Ok(fingerprint)
    }
}

pub struct Checkpointer {
    directory: PathBuf,
    glob_string: String,
    checkpoints: HashMap<FileFingerprint, FilePosition>,
}

impl Checkpointer {
    pub fn new(data_dir: &Path) -> Checkpointer {
        let directory = data_dir.join("checkpoints");
        let glob_string = directory.join("*").to_string_lossy().into_owned();
        Checkpointer {
            directory: directory,
            glob_string: glob_string,
            checkpoints: HashMap::new(),
        }
    }

    fn encode(&self, fng: FileFingerprint, pos: FilePosition) -> PathBuf {
        self.directory.join(format!("{:x}.{}", fng, pos))
    }
    fn decode(&self, path: &Path) -> (FileFingerprint, FilePosition) {
        let file_name = &path.file_name().unwrap().to_string_lossy();
        scan_fmt!(file_name, "{x}.{}", [hex FileFingerprint], FilePosition).unwrap()
    }

    pub fn set_checkpoint(&mut self, fng: FileFingerprint, pos: FilePosition) {
        self.checkpoints.insert(fng, pos);
    }

    pub fn get_checkpoint(&self, fng: FileFingerprint) -> Option<FilePosition> {
        self.checkpoints.get(&fng).cloned()
    }

    pub fn write_checkpoints(&mut self) -> Result<(), io::Error> {
        fs::remove_dir_all(&self.directory).ok();
        fs::create_dir_all(&self.directory)?;
        for (&fng, &pos) in self.checkpoints.iter() {
            fs::File::create(self.encode(fng, pos))?;
        }
        Ok(())
    }

    pub fn read_checkpoints(&mut self, ignore_before: Option<time::SystemTime>) {
        for path in glob(&self.glob_string).unwrap().flatten() {
            if let Some(ignore_before) = ignore_before {
                if let Ok(Ok(modified)) = fs::metadata(&path).map(|metadata| metadata.modified()) {
                    if modified < ignore_before {
                        fs::remove_file(path).ok();
                        continue;
                    }
                }
            }
            let (fng, pos) = self.decode(&path);
            self.checkpoints.insert(fng, pos);
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Checkpointer, FileFingerprint, FilePosition, FileServer};
    use std::{fs, time};
    use tempfile::tempdir;

    #[test]
    fn test_fingerprinting() {
        let data_dir = tempdir().unwrap();
        let target_dir = tempdir().unwrap();
        let file_server = FileServer {
            include: vec![target_dir.path().to_owned()],
            exclude: Vec::new(),
            max_read_bytes: 10000,
            start_at_beginning: true,
            ignore_before: None,
            max_line_bytes: 10000,
            fingerprint_bytes: 256,
            ignored_header_bytes: 0,
            data_dir: data_dir.path().to_owned(),
            glob_minimum_cooldown: time::Duration::from_millis(5),
        };

        let enough_data = vec![b'x'; 256];
        let not_enough_data = vec![b'x'; 199];
        let empty_path = target_dir.path().join("empty.log");
        let big_enough_path = target_dir.path().join("big_enough.log");
        let not_big_enough_path = target_dir.path().join("not_big_enough.log");
        fs::write(&empty_path, &[]).unwrap();
        fs::write(&big_enough_path, &enough_data).unwrap();
        fs::write(&not_big_enough_path, &not_enough_data).unwrap();

        let mut buf = Vec::new();
        assert!(file_server
            .get_fingerprint_of_file(&empty_path, &mut buf)
            .is_err());
        assert!(file_server
            .get_fingerprint_of_file(&big_enough_path, &mut buf)
            .is_ok());
        assert!(file_server
            .get_fingerprint_of_file(&not_big_enough_path, &mut buf)
            .is_err());
    }

    #[test]
    fn test_checkpointer_basics() {
        let fingerprint: FileFingerprint = 0x1234567890abcdef;
        let position: FilePosition = 1234;
        let data_dir = tempdir().unwrap();
        let mut chkptr = Checkpointer::new(&data_dir.path());
        assert_eq!(
            chkptr.decode(&chkptr.encode(fingerprint, position)),
            (fingerprint, position)
        );
        chkptr.set_checkpoint(fingerprint, position);
        assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
    }

    #[test]
    fn test_checkpointer_restart() {
        let fingerprint: FileFingerprint = 0x1234567890abcdef;
        let position: FilePosition = 1234;
        let data_dir = tempdir().unwrap();
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            chkptr.set_checkpoint(fingerprint, position);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
            chkptr.write_checkpoints().ok();
        }
        {
            let mut chkptr = Checkpointer::new(&data_dir.path());
            assert_eq!(chkptr.get_checkpoint(fingerprint), None);
            chkptr.read_checkpoints(None);
            assert_eq!(chkptr.get_checkpoint(fingerprint), Some(position));
        }
    }
}

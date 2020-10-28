use crate::{file_watcher::FileWatcher, FileFingerprint, FilePosition, FileSourceInternalEvents};
use bytes::Bytes;
use futures::{
    executor::block_on,
    future::{select, Either},
    stream, Future, Sink, SinkExt,
};
use glob::glob;
use indexmap::IndexMap;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::{self, remove_file, File};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time::{self, Duration};
use tokio::time::delay_for;

use crate::metadata_ext::PortableFileExt;
use crate::paths_provider::PathsProvider;

/// `FileServer` is a Source which cooperatively schedules reads over files,
/// converting the lines of said files into `LogLine` structures. As
/// `FileServer` is intended to be useful across multiple operating systems with
/// POSIX filesystem semantics `FileServer` must poll for changes. That is, no
/// event notification is used by `FileServer`.
///
/// `FileServer` is configured on a path to watch. The files do _not_ need to
/// exist at startup. `FileServer` will discover new files which match
/// its path in at most 60 seconds.
pub struct FileServer<PP, E: FileSourceInternalEvents>
where
    PP: PathsProvider,
{
    pub paths_provider: PP,
    pub max_read_bytes: usize,
    pub start_at_beginning: bool,
    pub ignore_before: Option<time::SystemTime>,
    pub max_line_bytes: usize,
    pub data_dir: PathBuf,
    pub glob_minimum_cooldown: Duration,
    pub fingerprinter: Fingerprinter,
    pub oldest_first: bool,
    pub remove_after: Option<Duration>,
    pub emitter: E,
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
/// rare occurrence.
///
/// Specific operating systems support evented interfaces that correct this
/// problem but your intrepid authors know of no generic solution.
impl<PP, E> FileServer<PP, E>
where
    PP: PathsProvider,
    E: FileSourceInternalEvents,
{
    pub fn run<C>(
        self,
        mut chans: C,
        mut shutdown: impl Future + Unpin,
    ) -> Result<Shutdown, <C as Sink<Vec<(Bytes, String)>>>::Error>
    where
        C: Sink<Vec<(Bytes, String)>> + Unpin,
        <C as Sink<Vec<(Bytes, String)>>>::Error: std::error::Error,
    {
        let mut fingerprint_buffer = Vec::new();

        let mut fp_map: IndexMap<FileFingerprint, FileWatcher> = Default::default();

        let mut backoff_cap: usize = 1;
        let mut lines = Vec::new();

        let mut checkpointer = Checkpointer::new(&self.data_dir);
        checkpointer.read_checkpoints(self.ignore_before);

        let mut known_small_files = HashSet::new();

        let mut existing_files = Vec::new();
        for path in self.paths_provider.paths().into_iter() {
            if let Some(file_id) = self.fingerprinter.get_fingerprint_or_log_error(
                &path,
                &mut fingerprint_buffer,
                &mut known_small_files,
                &self.emitter,
            ) {
                existing_files.push((path, file_id));
            }
        }

        existing_files.sort_by_key(|(path, _file_id)| {
            fs::metadata(&path)
                .and_then(|m| m.created())
                .unwrap_or_else(|_| time::SystemTime::now())
        });

        for (path, file_id) in existing_files {
            self.watch_new_file(
                path,
                file_id,
                &mut fp_map,
                &checkpointer,
                self.start_at_beginning,
            );
        }

        let mut stats = TimingStats::default();

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

                if stats.started_at.elapsed() > Duration::from_secs(1) {
                    stats.report();
                }

                if stats.started_at.elapsed() > Duration::from_secs(10) {
                    stats = TimingStats::default();
                }

                let start = time::Instant::now();
                // Write any stored checkpoints (uses glob to find old checkpoints).
                checkpointer
                    .write_checkpoints()
                    .map_err(|error| self.emitter.emit_file_checkpoint_write_failed(error))
                    .map(|count| self.emitter.emit_file_checkpointed(count))
                    .ok();
                stats.record("checkpointing", start.elapsed());

                // Search (glob) for files to detect major file changes.
                let start = time::Instant::now();
                for (_file_id, watcher) in &mut fp_map {
                    watcher.set_file_findable(false); // assume not findable until found
                }
                for path in self.paths_provider.paths().into_iter() {
                    if let Some(file_id) = self.fingerprinter.get_fingerprint_or_log_error(
                        &path,
                        &mut fingerprint_buffer,
                        &mut known_small_files,
                        &self.emitter,
                    ) {
                        if let Some(watcher) = fp_map.get_mut(&file_id) {
                            // file fingerprint matches a watched file
                            let was_found_this_cycle = watcher.file_findable();
                            watcher.set_file_findable(true);
                            if watcher.path == path {
                                trace!(
                                    message = "Continue watching file.",
                                    path = ?path,
                                );
                            } else {
                                // matches a file with a different path
                                if !was_found_this_cycle {
                                    info!(
                                        message = "Watched file has been renamed.",
                                        path = ?path,
                                        old_path = ?watcher.path
                                    );
                                    watcher.update_path(path).ok(); // ok if this fails: might fix next cycle
                                } else {
                                    info!(
                                        message = "More than one file has the same fingerprint.",
                                        path = ?path,
                                        old_path = ?watcher.path
                                    );
                                    let (old_path, new_path) = (&watcher.path, &path);
                                    if let (Ok(old_modified_time), Ok(new_modified_time)) = (
                                        fs::metadata(&old_path).and_then(|m| m.modified()),
                                        fs::metadata(&new_path).and_then(|m| m.modified()),
                                    ) {
                                        if old_modified_time < new_modified_time {
                                            info!(
                                                message = "Switching to watch most recently modified file.",
                                                new_modified_time = ?new_modified_time,
                                                old_modified_time = ?old_modified_time,
                                            );
                                            watcher.update_path(path).ok(); // ok if this fails: might fix next cycle
                                        }
                                    }
                                }
                            }
                        } else {
                            // untracked file fingerprint
                            self.watch_new_file(path, file_id, &mut fp_map, &checkpointer, false);
                        }
                    }
                }
                stats.record("discovery", start.elapsed());
            }

            // Collect lines by polling files.
            let mut global_bytes_read: usize = 0;
            let mut maxed_out_reading_single_file = false;
            for (&file_id, watcher) in &mut fp_map {
                if !watcher.should_read() {
                    continue;
                }

                let start = time::Instant::now();
                let mut bytes_read: usize = 0;
                while let Ok(Some(line)) = watcher.read_line() {
                    if line.is_empty() {
                        break;
                    }

                    let sz = line.len();
                    trace!(
                        message = "Read bytes.",
                        path = ?watcher.path,
                        bytes = ?sz
                    );
                    stats.record_bytes(sz);

                    bytes_read += sz;

                    lines.push((
                        line,
                        watcher.path.to_str().expect("not a valid path").to_owned(),
                    ));

                    if bytes_read > self.max_read_bytes {
                        maxed_out_reading_single_file = true;
                        break;
                    }
                }
                stats.record("reading", start.elapsed());

                if bytes_read > 0 {
                    global_bytes_read = global_bytes_read.saturating_add(bytes_read);
                    checkpointer.set_checkpoint(file_id, watcher.get_file_position());
                } else {
                    // Should the file be removed
                    if let Some(grace_period) = self.remove_after {
                        if watcher.last_read_success().elapsed() >= grace_period {
                            // Try to remove
                            match remove_file(&watcher.path) {
                                Ok(()) => {
                                    self.emitter.emit_file_deleted(&watcher.path);
                                    watcher.set_dead();
                                }
                                Err(error) => {
                                    // We will try again after some time.
                                    self.emitter.emit_file_delete_failed(&watcher.path, error);
                                }
                            }
                        }
                    }
                }

                // Do not move on to newer files if we are behind on an older file
                if self.oldest_first && maxed_out_reading_single_file {
                    break;
                }
            }

            // A FileWatcher is dead when the underlying file has disappeared.
            // If the FileWatcher is dead we don't retain it; it will be deallocated.
            fp_map.retain(|_file_id, watcher| {
                if watcher.dead() {
                    self.emitter.emit_file_unwatched(&watcher.path);
                    false
                } else {
                    true
                }
            });

            let start = time::Instant::now();
            let to_send = std::mem::take(&mut lines);
            let mut stream = stream::once(futures::future::ok(to_send));
            let result = block_on(chans.send_all(&mut stream));
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!(message = "Output channel closed.", error = ?error);
                    return Err(error);
                }
            }
            stats.record("sending", start.elapsed());

            let start = time::Instant::now();
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

            // This works only if run inside tokio context since we are using
            // tokio's Timer. Outside of such context, this will panic on the first
            // call. Also since we are using block_on here and in the above code,
            // this should be run in it's own thread. `spawn_blocking` fulfills
            // all of these requirements.
            let sleep = async move {
                if backoff > 0 {
                    delay_for(Duration::from_millis(backoff as u64)).await;
                }
            };
            futures::pin_mut!(sleep);
            match block_on(select(shutdown, sleep)) {
                Either::Left((_, _)) => return Ok(Shutdown),
                Either::Right((_, future)) => shutdown = future,
            }
            stats.record("sleeping", start.elapsed());
        }
    }

    fn watch_new_file(
        &self,
        path: PathBuf,
        file_id: FileFingerprint,
        fp_map: &mut IndexMap<FileFingerprint, FileWatcher>,
        checkpointer: &Checkpointer,
        read_from_beginning: bool,
    ) {
        let file_position = if read_from_beginning {
            0
        } else {
            checkpointer.get_checkpoint(file_id).unwrap_or(0)
        };
        match FileWatcher::new(
            path.clone(),
            file_position,
            self.ignore_before,
            self.max_line_bytes,
        ) {
            Ok(mut watcher) => {
                if file_position == 0 {
                    self.emitter.emit_file_added(&path);
                } else {
                    self.emitter.emit_file_resumed(&path, file_position);
                }
                watcher.set_file_findable(true);
                fp_map.insert(file_id, watcher);
            }
            Err(error) => self.emitter.emit_file_watch_failed(&path, error),
        };
    }
}

/// A sentinel type to signal that file server was gracefully shut down.
///
/// The purpose of this type is to clarify the semantics of the result values
/// returned from the [`FileServer::run`] for both the users of the file server,
/// and the implementors.
#[derive(Debug)]
pub struct Shutdown;

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
            directory,
            glob_string,
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

    pub fn write_checkpoints(&mut self) -> Result<usize, io::Error> {
        fs::remove_dir_all(&self.directory).ok();
        fs::create_dir_all(&self.directory)?;
        for (&fng, &pos) in self.checkpoints.iter() {
            fs::File::create(self.encode(fng, pos))?;
        }
        Ok(self.checkpoints.len())
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

#[derive(Clone)]
pub enum Fingerprinter {
    Checksum {
        bytes: usize,
        ignored_header_bytes: usize,
    },
    FirstLineChecksum {
        max_line_length: usize,
    },
    DevInode,
}

impl Fingerprinter {
    fn get_fingerprint_of_file(
        &self,
        path: &PathBuf,
        buffer: &mut Vec<u8>,
    ) -> Result<FileFingerprint, io::Error> {
        match *self {
            Fingerprinter::DevInode => {
                let file_handle = File::open(path)?;
                let dev = file_handle.portable_dev()?;
                let ino = file_handle.portable_ino()?;
                buffer.clear();
                buffer.write_all(&dev.to_be_bytes())?;
                buffer.write_all(&ino.to_be_bytes())?;
            }
            Fingerprinter::Checksum {
                ignored_header_bytes,
                bytes,
            } => {
                let i = ignored_header_bytes as u64;
                let b = bytes;
                buffer.resize(b, 0u8);
                let mut fp = fs::File::open(path)?;
                fp.seek(io::SeekFrom::Start(i))?;
                fp.read_exact(&mut buffer[..b])?;
            }
            Fingerprinter::FirstLineChecksum { max_line_length } => {
                buffer.resize(max_line_length, 0u8);
                let fp = fs::File::open(path)?;
                fingerprinter_read_until(fp, b'\n', buffer)?;
            }
        }
        let fingerprint = crc::crc64::checksum_ecma(&buffer[..]);
        Ok(fingerprint)
    }

    fn get_fingerprint_or_log_error(
        &self,
        path: &PathBuf,
        buffer: &mut Vec<u8>,
        known_small_files: &mut HashSet<PathBuf>,
        emitter: &impl FileSourceInternalEvents,
    ) -> Option<FileFingerprint> {
        self.get_fingerprint_of_file(path, buffer)
            .map_err(|error| {
                if error.kind() == io::ErrorKind::UnexpectedEof {
                    if !known_small_files.contains(path) {
                        emitter.emit_file_checksum_failed(path);
                        known_small_files.insert(path.clone());
                    }
                } else {
                    emitter.emit_file_fingerprint_read_failed(path, error);
                }
            })
            .ok()
    }
}

fn fingerprinter_read_until(mut r: impl Read, delim: u8, mut buf: &mut [u8]) -> io::Result<()> {
    while !buf.is_empty() {
        let read = match r.read(buf) {
            Ok(0) => return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "EOF reached")),
            Ok(n) => n,
            Err(ref e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };

        if let Some((pos, _)) = buf[..read].iter().enumerate().find(|(_, &c)| c == delim) {
            for el in &mut buf[(pos + 1)..] {
                *el = 0;
            }
            break;
        }

        buf = &mut buf[read..];
    }
    Ok(())
}

struct TimingStats {
    started_at: time::Instant,
    segments: BTreeMap<&'static str, Duration>,
    events: usize,
    bytes: usize,
}

impl TimingStats {
    fn record(&mut self, key: &'static str, duration: Duration) {
        let segment = self.segments.entry(key).or_default();
        *segment += duration;
    }

    fn record_bytes(&mut self, bytes: usize) {
        self.events += 1;
        self.bytes += bytes;
    }

    fn report(&self) {
        let total = self.started_at.elapsed();
        let counted = self.segments.values().sum();
        let other = self.started_at.elapsed() - counted;
        let mut ratios = self
            .segments
            .iter()
            .map(|(k, v)| (*k, v.as_secs_f32() / total.as_secs_f32()))
            .collect::<BTreeMap<_, _>>();
        ratios.insert("other", other.as_secs_f32() / total.as_secs_f32());
        let (event_throughput, bytes_throughput) = if total.as_secs() > 0 {
            (
                self.events as u64 / total.as_secs(),
                self.bytes as u64 / total.as_secs(),
            )
        } else {
            (0, 0)
        };
        debug!(event_throughput = %scale(event_throughput), bytes_throughput = %scale(bytes_throughput), ?ratios);
    }
}

fn scale(bytes: u64) -> String {
    let units = ["", "k", "m", "g"];
    let mut bytes = bytes as f32;
    let mut i = 0;
    while bytes > 1000.0 && i <= 3 {
        bytes /= 1000.0;
        i += 1;
    }
    format!("{:.3}{}/sec", bytes, units[i])
}

impl Default for TimingStats {
    fn default() -> Self {
        Self {
            started_at: time::Instant::now(),
            segments: Default::default(),
            events: Default::default(),
            bytes: Default::default(),
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Checkpointer, FileFingerprint, FilePosition, Fingerprinter};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_checksum_fingerprint() {
        let fingerprinter = Fingerprinter::Checksum {
            bytes: 256,
            ignored_header_bytes: 0,
        };

        let target_dir = tempdir().unwrap();
        let enough_data = vec![b'x'; 256];
        let not_enough_data = vec![b'x'; 199];
        let empty_path = target_dir.path().join("empty.log");
        let big_enough_path = target_dir.path().join("big_enough.log");
        let duplicate_path = target_dir.path().join("duplicate.log");
        let not_big_enough_path = target_dir.path().join("not_big_enough.log");
        fs::write(&empty_path, &[]).unwrap();
        fs::write(&big_enough_path, &enough_data).unwrap();
        fs::write(&duplicate_path, &enough_data).unwrap();
        fs::write(&not_big_enough_path, &not_enough_data).unwrap();

        let mut buf = Vec::new();
        assert!(fingerprinter
            .get_fingerprint_of_file(&empty_path, &mut buf)
            .is_err());
        assert!(fingerprinter
            .get_fingerprint_of_file(&big_enough_path, &mut buf)
            .is_ok());
        assert!(fingerprinter
            .get_fingerprint_of_file(&not_big_enough_path, &mut buf)
            .is_err());
        assert_eq!(
            fingerprinter
                .get_fingerprint_of_file(&big_enough_path, &mut buf)
                .unwrap(),
            fingerprinter
                .get_fingerprint_of_file(&duplicate_path, &mut buf)
                .unwrap(),
        );
    }

    #[test]
    fn test_first_line_checksum_fingerprint() {
        let max_line_length = 64;
        let fingerprinter = Fingerprinter::FirstLineChecksum { max_line_length };

        let target_dir = tempdir().unwrap();
        let prepare_test = |file: &str, contents: &[u8]| {
            let path = target_dir.path().join(file);
            fs::write(&path, contents).unwrap();
            path
        };
        let prepare_test_long = |file: &str, amount| {
            prepare_test(
                file,
                b"hello world "
                    .iter()
                    .cloned()
                    .cycle()
                    .clone()
                    .take(amount)
                    .collect::<Box<_>>()
                    .as_ref(),
            )
        };

        let empty = prepare_test("empty.log", b"");
        let incomlete_line = prepare_test("incomlete_line.log", b"missing newline char");
        let one_line = prepare_test("one_line.log", b"hello world\n");
        let one_line_duplicate = prepare_test("one_line_duplicate.log", b"hello world\n");
        let one_line_continued =
            prepare_test("one_line_continued.log", b"hello world\nthe next line\n");
        let different_two_lines = prepare_test("different_two_lines.log", b"line one\nline two\n");

        let exactly_max_line_length =
            prepare_test_long("exactly_max_line_length.log", max_line_length);
        let exceeding_max_line_length =
            prepare_test_long("exceeding_max_line_length.log", max_line_length + 1);
        let incomplete_under_max_line_length_by_one = prepare_test_long(
            "incomplete_under_max_line_length_by_one.log",
            max_line_length - 1,
        );

        let mut buf = Vec::new();
        let mut run = move |path| fingerprinter.get_fingerprint_of_file(path, &mut buf);

        assert!(run(&empty).is_err());
        assert!(run(&incomlete_line).is_err());
        assert!(run(&incomplete_under_max_line_length_by_one).is_err());

        assert!(run(&one_line).is_ok());
        assert!(run(&one_line_duplicate).is_ok());
        assert!(run(&one_line_continued).is_ok());
        assert!(run(&different_two_lines).is_ok());
        assert!(run(&exactly_max_line_length).is_ok());
        assert!(run(&exceeding_max_line_length).is_ok());

        assert_eq!(run(&one_line).unwrap(), run(&one_line_duplicate).unwrap());
        assert_eq!(run(&one_line).unwrap(), run(&one_line_continued).unwrap());

        assert_ne!(run(&one_line).unwrap(), run(&different_two_lines).unwrap());

        assert_eq!(
            run(&exactly_max_line_length).unwrap(),
            run(&exceeding_max_line_length).unwrap()
        );
    }

    #[test]
    fn test_inode_fingerprint() {
        let fingerprinter = Fingerprinter::DevInode;

        let target_dir = tempdir().unwrap();
        let small_data = vec![b'x'; 1];
        let medium_data = vec![b'x'; 256];
        let empty_path = target_dir.path().join("empty.log");
        let small_path = target_dir.path().join("small.log");
        let medium_path = target_dir.path().join("medium.log");
        let duplicate_path = target_dir.path().join("duplicate.log");
        fs::write(&empty_path, &[]).unwrap();
        fs::write(&small_path, &small_data).unwrap();
        fs::write(&medium_path, &medium_data).unwrap();
        fs::write(&duplicate_path, &medium_data).unwrap();

        let mut buf = Vec::new();
        assert!(fingerprinter
            .get_fingerprint_of_file(&empty_path, &mut buf)
            .is_ok());
        assert!(fingerprinter
            .get_fingerprint_of_file(&small_path, &mut buf)
            .is_ok());
        assert_ne!(
            fingerprinter
                .get_fingerprint_of_file(&medium_path, &mut buf)
                .unwrap(),
            fingerprinter
                .get_fingerprint_of_file(&duplicate_path, &mut buf)
                .unwrap()
        );
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

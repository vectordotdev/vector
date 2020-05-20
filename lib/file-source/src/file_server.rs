use crate::{file_watcher::FileWatcher, FileFingerprint, FilePosition};
use bytes::Bytes;
use futures::{
    executor::block_on,
    future::{select, Either},
    stream, Future, Sink, SinkExt,
};
use glob::glob;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::{self, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::time;
use tokio::time::delay_for;
use tracing::field;

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
pub struct FileServer<PP>
where
    PP: PathsProvider,
{
    pub paths_provider: PP,
    pub max_read_bytes: usize,
    pub start_at_beginning: bool,
    pub ignore_before: Option<time::SystemTime>,
    pub max_line_bytes: usize,
    pub data_dir: PathBuf,
    pub glob_minimum_cooldown: time::Duration,
    pub fingerprinter: Fingerprinter,
    pub oldest_first: bool,
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
impl<PP> FileServer<PP>
where
    PP: PathsProvider,
{
    pub fn run<C>(
        self,
        mut chans: C,
        mut shutdown: impl Future + Unpin,
    ) -> Result<Shutdown, <C as Sink<(Bytes, String)>>::Error>
    where
        C: Sink<(Bytes, String)> + Unpin,
        <C as Sink<(Bytes, String)>>::Error: std::error::Error,
    {
        let mut line_buffer = Vec::new();
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
                for (_file_id, watcher) in &mut fp_map {
                    watcher.set_file_findable(false); // assume not findable until found
                }
                for path in self.paths_provider.paths().into_iter() {
                    if let Some(file_id) = self.fingerprinter.get_fingerprint_or_log_error(
                        &path,
                        &mut fingerprint_buffer,
                        &mut known_small_files,
                    ) {
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
                            self.watch_new_file(path, file_id, &mut fp_map, &checkpointer, false);
                        }
                    }
                }
            }

            // Collect lines by polling files.
            let mut global_bytes_read: usize = 0;
            let mut maxed_out_reading_single_file = false;
            for (&file_id, watcher) in &mut fp_map {
                if !watcher.should_read() {
                    continue;
                }

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
                        maxed_out_reading_single_file = true;
                        break;
                    }
                }
                if bytes_read > 0 {
                    global_bytes_read = global_bytes_read.saturating_add(bytes_read);
                    checkpointer.set_checkpoint(file_id, watcher.get_file_position());
                }
                // Do not move on to newer files if we are behind on an older file
                if self.oldest_first && maxed_out_reading_single_file {
                    break;
                }
            }

            // A FileWatcher is dead when the underlying file has disappeared.
            // If the FileWatcher is dead we don't retain it; it will be deallocated.
            fp_map.retain(|_file_id, watcher| !watcher.dead());

            let mut stream = stream::iter(lines.drain(..).map(Ok));
            let result = block_on(chans.send_all(&mut stream));
            match result {
                Ok(()) => {}
                Err(error) => {
                    error!(message = "output channel closed", ?error);
                    return Err(error);
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

            // This works only if run inside tokio context since we are using
            // tokio's Timer. Outside of such context, this will panic on the first
            // call. Also since we are using block_on here and in the above code,
            // this should be run in it's own thread. `spawn_blocking` fulfills
            // all of these requirements.
            match block_on(select(
                shutdown,
                delay_for(time::Duration::from_millis(backoff as u64)),
            )) {
                Either::Left((_, _)) => return Ok(Shutdown),
                Either::Right((_, future)) => shutdown = future,
            }
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
        match FileWatcher::new(path.clone(), file_position, self.ignore_before) {
            Ok(mut watcher) => {
                info!(
                    message = "Found file to watch.",
                    path = field::debug(&watcher.path),
                    file_position = field::debug(&file_position),
                );
                watcher.set_file_findable(true);
                fp_map.insert(file_id, watcher);
            }
            Err(e) => error!(message = "Error watching new file", %e, file = ?path),
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

#[derive(Clone)]
pub enum Fingerprinter {
    Checksum {
        fingerprint_bytes: usize,
        ignored_header_bytes: usize,
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
                fingerprint_bytes,
            } => {
                let i = ignored_header_bytes as u64;
                let b = fingerprint_bytes;
                buffer.resize(b, 0u8);
                let mut fp = fs::File::open(path)?;
                fp.seek(io::SeekFrom::Start(i))?;
                fp.read_exact(&mut buffer[..b])?;
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
    ) -> Option<FileFingerprint> {
        self.get_fingerprint_of_file(path, buffer)
            .map_err(|err| {
                if err.kind() == io::ErrorKind::UnexpectedEof {
                    if !known_small_files.contains(path) {
                        warn!(message = "Ignoring file smaller than fingerprint_bytes", file = ?path);
                        known_small_files.insert(path.clone());
                    }
                } else {
                    error!(message = "Error reading file for fingerprinting", %err, file = ?path);
                }
            })
            .ok()
    }
}

#[cfg(test)]
mod test {
    use super::{Checkpointer, FileFingerprint, FilePosition, Fingerprinter};
    use std::fs;
    use tempfile::tempdir;

    #[test]
    fn test_checksum_fingerprinting() {
        let fingerprinter = Fingerprinter::Checksum {
            fingerprint_bytes: 256,
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
    fn test_inode_fingerprinting() {
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

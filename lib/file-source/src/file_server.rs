use crate::file_watcher::FileWatcher;
use bytes::Bytes;
use futures::{stream, Future, Sink, Stream};
use glob::{glob, Pattern};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
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
/// exist at cernan startup. `FileServer` will discover new files which match
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
}

type FileFingerprint = u64;

/// `FileServer` as Source
///
/// The 'run' of `FileServer` performs the cooperative scheduling of reads over
/// `FileServer`'s configured files. Much care has been taking to make this
/// scheduling 'fair', meaning busy files do not drown out quiet files or vice
/// versa but there's no one perfect approach. Very fast files _will_ be lost if
/// your system aggressively rolls log files. `FileServer` will keep a file
/// handler open but should your system move so quickly that a file disappears
/// before cernan is able to open it the contents will be lost. This should be a
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
        let mut line_buffer = Vec::new();
        let mut fingerprint_buffer = Vec::new();

        let mut fp_map: HashMap<FileFingerprint, FileWatcher> = Default::default();

        let mut backoff_cap: usize = 1;
        let mut lines = Vec::new();
        let mut start_of_run = true;
        // Alright friends, how does this work?
        //
        // We want to avoid burning up users' CPUs. To do this we sleep after
        // reading lines out of files. But! We want to be responsive as well. We
        // keep track of a 'backoff_cap' to decide how long we'll wait in any
        // given loop. This cap grows each time we fail to read lines in an
        // exponential fashion to some hard-coded cap.
        loop {
            let mut global_bytes_read: usize = 0;
            // glob poll
            let exclude_patterns = self
                .exclude
                .iter()
                .map(|e| Pattern::new(e.to_str().expect("no ability to glob")).unwrap())
                .collect::<Vec<_>>();
            for (_file_id, watcher) in &mut fp_map {
                watcher.set_file_findable(false); // assume not findable until found
            }
            for include_pattern in &self.include {
                for entry in glob(include_pattern.to_str().expect("no ability to glob"))
                    .expect("Failed to read glob pattern")
                {
                    if let Ok(path) = entry {
                        if exclude_patterns
                            .iter()
                            .any(|e| e.matches(path.to_str().unwrap()))
                        {
                            continue;
                        }

                        if let Some(file_id) =
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
                                // unknown (new) file fingerprint
                                let read_file_from_beginning = if start_of_run {
                                    self.start_at_beginning
                                } else {
                                    true
                                };
                                if let Ok(mut watcher) = FileWatcher::new(
                                    path,
                                    read_file_from_beginning,
                                    self.ignore_before,
                                ) {
                                    info!(
                                        message = "Found file to watch.",
                                        path = field::debug(&watcher.path),
                                        start_at_beginning = field::debug(&self.start_at_beginning),
                                        start_of_run = field::debug(&start_of_run),
                                    );
                                    watcher.set_file_findable(true);
                                    fp_map.insert(file_id, watcher);
                                };
                            }
                        }
                    }
                }
            }
            // line polling
            for (_file_id, watcher) in &mut fp_map {
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
                global_bytes_read = global_bytes_read.saturating_add(bytes_read);
            }

            // A FileWatcher is dead when the underlying file has disappeared.
            // If the FileWatcher is dead we don't retain it; it will be deallocated.
            fp_map.retain(|_file_id, watcher| !watcher.dead());

            match stream::iter_ok::<_, ()>(lines.drain(..))
                .forward(chans)
                .wait()
            {
                Ok((_, sink)) => chans = sink,
                Err(_) => unreachable!("Output channel is closed"),
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

            start_of_run = false;
        }
    }

    fn get_fingerprint_of_file(
        &self,
        path: &PathBuf,
        buffer: &mut Vec<u8>,
    ) -> Option<FileFingerprint> {
        let i = self.ignored_header_bytes as u64;
        let b = self.fingerprint_bytes;
        buffer.resize(b, 0u8);
        if let Ok(mut fp) = fs::File::open(path) {
            if fp.seek(SeekFrom::Start(i)).is_ok() && fp.read_exact(&mut buffer[..b]).is_ok() {
                let fingerprint = crc::crc64::checksum_ecma(&buffer[..b]);
                Some(fingerprint)
            } else {
                None
            }
        } else {
            None
        }
    }
}

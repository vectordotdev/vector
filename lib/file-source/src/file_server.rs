use crate::file_watcher::FileWatcher;
use bytes::Bytes;
use futures::{stream, Future, Sink, Stream};
use glob::{glob, Pattern};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::PathBuf;
use std::sync::mpsc::RecvTimeoutError;
use std::time;
use tokio_trace::field;

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
}

type FileFingerprint = u64;

#[inline]
fn file_id(path: &PathBuf) -> Option<FileFingerprint> {
    if let Ok(mut f) = fs::File::open(path) {
        let mut header = [0; 256];
        if let Ok(_) = f.read_exact(&mut header) {
            let fingerprint = crc::crc64::checksum_ecma(&header[..]);
            let metadata = f.metadata().unwrap();
            Some(fingerprint)
        } else {
            None
        }
    } else {
        None
    }
}

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
        let mut buffer = Vec::new();

        let mut fp_map: HashMap<FileFingerprint, FileWatcher> = Default::default();

        let mut backoff_cap: usize = 1;
        let mut lines = Vec::new();
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
            for (_file_id, watcher) in fp_map.iter_mut() {
                watcher.listed = false;
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

                        if let Some(file_id) = file_id(&path) {
                            if fp_map.contains_key(&file_id) {
                                let watcher = fp_map.get_mut(&file_id).unwrap();
                                watcher.listed = true;
                                if watcher.path != path {
                                    info!(
                                        message = "Watched file has been renamed.",
                                        path = field::debug(&path),
                                        old_path = field::debug(&watcher.path)
                                    );
                                    watcher.path = path.clone();
                                }
                            } else {
                                if let Ok(watcher) = FileWatcher::new(
                                    &path,
                                    self.start_at_beginning,
                                    self.ignore_before,
                                ) {
                                    info!(
                                        message = "Found file to watch.",
                                        path = field::debug(&path),
                                        start_at_beginning = field::debug(&self.start_at_beginning)
                                    );
                                    fp_map.insert(file_id, watcher);
                                };
                            }
                        }
                    }
                }
            }
            // line polling
            for (_file_id, watcher) in fp_map.iter_mut() {
                let mut bytes_read: usize = 0;
                while let Ok(sz) = watcher.read_line(&mut buffer, self.max_line_bytes) {
                    if sz > 0 {
                        trace!(
                            message = "Read bytes.",
                            path = field::debug(&watcher.path),
                            bytes = field::debug(sz)
                        );

                        bytes_read += sz;

                        if !buffer.is_empty() {
                            lines.push((
                                buffer.clone().into(),
                                watcher.path.to_str().expect("not a valid path").to_owned(),
                            ));
                            buffer.clear();
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
        }
    }
}

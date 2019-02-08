use crate::file_watcher::FileWatcher;
use glob::glob;
use std::mem;
use std::path::PathBuf;
use std::time;
use std::collections::HashMap;
use futures::{stream, Stream, Future, Sink};
use std::sync::mpsc::RecvTimeoutError;

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
    pub max_read_bytes: usize,
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
    pub fn run(self, mut chans: impl Sink<SinkItem=(String, String), SinkError=()>, shutdown: std::sync::mpsc::Receiver<()>) {
        let mut buffer = String::new();

        let mut fp_map: HashMap<PathBuf, FileWatcher> = Default::default();
        let mut fp_map_alt: HashMap<PathBuf, FileWatcher> = Default::default();

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
            for path in &self.include {
                for entry in glob(path.to_str().expect("no ability to glob"))
                    .expect("Failed to read glob pattern")
                {
                    if let Ok(path) = entry {
                        let entry = fp_map.entry(path.clone());
                        if let Ok(fw) = FileWatcher::new(&path) {
                            entry.or_insert(fw);
                        };
                    }
                }
            }
            // line polling
            for (path, mut watcher) in fp_map.drain() {
                let mut bytes_read: usize = 0;
                while let Ok(sz) = watcher.read_line(&mut buffer) {
                    if sz > 0 {
                        bytes_read += sz;
                        lines.push((buffer.clone(), path.to_str().expect("not a valid path").to_owned()));
                        buffer.clear();
                    } else {
                        break;
                    }
                    if bytes_read > self.max_read_bytes {
                        break;
                    }
                }
                // A FileWatcher is dead when the underlying file has
                // disappeared. If the FileWatcher is dead we don't stick it in
                // the fp_map_alt and deallocate it.
                if !watcher.dead() {
                    fp_map_alt.insert(path, watcher);
                }
                global_bytes_read = global_bytes_read.saturating_add(bytes_read);
            }

            match stream::iter_ok::<_, ()>(lines.drain(..)).forward(chans).wait() {
                Ok((_, sink)) => chans = sink,
                Err(_) => unreachable!(),
            }
            // We've drained the live FileWatchers into fp_map_alt in the line
            // polling loop. Now we swapped them back to fp_map so next time we
            // loop through we'll read from the live FileWatchers.
            mem::swap(&mut fp_map, &mut fp_map_alt);
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
                Err(RecvTimeoutError::Timeout) => {},
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
    }
}

// use crate::metric;
// use crate::source;
use crate::file_watcher::FileWatcher;
// use crate::source::internal::report_full_telemetry;
// use crate::util;
// use crate::util::send;
use glob::glob;
use mio;
use std::mem;
use std::path::PathBuf;
use std::str;
use std::time;

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
    pattern: PathBuf,
    max_read_bytes: usize,
}

/// The configuration struct for `FileServer`.
#[derive(Clone, Debug, Deserialize)]
pub struct FileServerConfig {
    /// The path that `FileServer` will watch. Globs are allowed and
    /// `FileServer` will watch multiple files.
    pub path: Option<PathBuf>,
    /// The maximum number of bytes to read from a file before switching to a
    /// new file.
    pub max_read_bytes: usize,
    /// The forwards which `FileServer` will obey.
    pub forwards: Vec<String>,
    /// The configured name of FileServer.
    pub config_path: Option<String>,
}

impl Default for FileServerConfig {
    fn default() -> Self {
        FileServerConfig {
            path: None,
            max_read_bytes: 2048,
            forwards: Vec::default(),
            config_path: None,
        }
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
impl source::Source<FileServerConfig> for FileServer {
    /// Make a FileServer
    fn init(config: FileServerConfig) -> Self {
        let pattern = config.path.expect("must specify a 'path' for FileServer");
        FileServer {
            pattern: pattern,
            max_read_bytes: config.max_read_bytes,
        }
    }

    fn run(self, mut chans: util::Channel, poller: mio::Poll) {
        let mut buffer = String::new();

        let mut fp_map: util::HashMap<PathBuf, FileWatcher> = Default::default();
        let mut fp_map_alt: util::HashMap<PathBuf, FileWatcher> = Default::default();

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
            for entry in glob(self.pattern.to_str().expect("no ability to glob"))
                .expect("Failed to read glob pattern")
            {
                if let Ok(path) = entry {
                    let entry = fp_map.entry(path.clone());
                    if let Ok(fw) = FileWatcher::new(&path) {
                        entry.or_insert(fw);
                    };
                }
            }
            // line polling
            for (path, mut watcher) in fp_map.drain() {
                let mut bytes_read: usize = 0;
                while let Ok(sz) = watcher.read_line(&mut buffer) {
                    if sz > 0 {
                        bytes_read += sz;
                        lines.push(metric::LogLine::new(
                            path.to_str().expect("not a valid path"),
                            &buffer,
                        ));
                        buffer.clear();
                    } else {
                        break;
                    }
                    if bytes_read > self.max_read_bytes {
                        break;
                    }
                }
                report_full_telemetry(
                    "cernan.sources.file.bytes_read",
                    bytes_read as f64,
                    Some(vec![(
                        "file_path",
                        path.to_str().expect("not a valid path"),
                    )]),
                );
                // A FileWatcher is dead when the underlying file has
                // disappeared. If the FileWatcher is dead we don't stick it in
                // the fp_map_alt and deallocate it.
                if !watcher.dead() {
                    fp_map_alt.insert(path, watcher);
                }
                global_bytes_read = global_bytes_read.saturating_add(bytes_read);
            }
            for l in lines.drain(..) {
                send(&mut chans, metric::Event::new_log(l));
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
            let mut events = mio::Events::with_capacity(1024);
            match poller.poll(
                &mut events,
                Some(time::Duration::from_millis(backoff as u64)),
            ) {
                Err(e) => panic!(format!("Failed during poll {:?}", e)),
                Ok(0) => {}
                Ok(_num_events) => {
                    // File server doesn't poll for anything other than SYSTEM events.
                    // As currently there are no system events other than SHUTDOWN,
                    // we immediately exit.
                    send(&mut chans, metric::Event::Shutdown);
                    return;
                }
            }
        }
    }
}

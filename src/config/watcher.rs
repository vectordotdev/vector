use std::{
    path::{Path, PathBuf},
    time::Duration,
};
use std::{
    sync::mpsc::{channel, Receiver},
    thread,
};

use notify::{recommended_watcher, EventKind, RecursiveMode};

use crate::Error;

/// Per notify own documentation, it's advised to have delay of more than 30 sec,
/// so to avoid receiving repetitions of previous events on macOS.
///
/// But, config and topology reload logic can handle:
///  - Invalid config, caused either by user or by data race.
///  - Frequent changes, caused by user/editor modifying/saving file in small chunks.
/// so we can use smaller, more responsive delay.
const CONFIG_WATCH_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

const RETRY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

pub enum WatcherConfig {
    /// Recommended watcher for the current OS, usually `inotify` for Linux-based systems.
    RecommendedWatcher,
    /// Poll-based watcher, typically used for watching files on NFS.
    PollWatcher(u64),
}

enum Watcher {
    /// recommended watcher for os, usually inotify for linux based systems
    RecommendedWatcher(notify::RecommendedWatcher),
    /// poll based watcher. for watching files from NFS.
    PollWatcher(notify::PollWatcher),
}

impl Watcher {
    fn add_paths(&mut self, config_paths: &[PathBuf]) -> Result<(), Error> {
        for path in config_paths {
            self.watch(path, RecursiveMode::Recursive)?;
        }
        Ok(())
    }

    fn watch(&mut self, path: &Path, recursive_mode: RecursiveMode) -> Result<(), Error> {
        use notify::Watcher as NotifyWatcher;
        match self {
            Watcher::RecommendedWatcher(watcher) => {
                watcher.watch(path, recursive_mode)?;
            }
            Watcher::PollWatcher(watcher) => {
                watcher.watch(path, recursive_mode)?;
            }
        }
        Ok(())
    }
}

/// Sends a ReloadFromDisk on config_path changes.
/// Accumulates file changes until no change for given duration has occurred.
/// Has best effort guarantee of detecting all file changes from the end of
/// this function until the main thread stops.
pub fn spawn_thread<'a>(
    watcher_conf: WatcherConfig,
    signal_tx: crate::signal::SignalTx,
    config_paths: impl IntoIterator<Item = &'a PathBuf> + 'a,
    delay: impl Into<Option<Duration>>,
) -> Result<(), Error> {
    let config_paths: Vec<_> = config_paths.into_iter().cloned().collect();
    let delay = delay.into().unwrap_or(CONFIG_WATCH_DELAY);

    // Create watcher now so not to miss any changes happening between
    // returning from this function and the thread starting.
    let mut watcher = Some(create_watcher(&watcher_conf, &config_paths)?);

    info!("Watching configuration files.");

    thread::spawn(move || loop {
        if let Some((mut watcher, receiver)) = watcher.take() {
            while let Ok(Ok(event)) = receiver.recv() {
                if matches!(
                    event.kind,
                    EventKind::Create(_) | EventKind::Remove(_) | EventKind::Modify(_)
                ) {
                    debug!(message = "Configuration file change detected.", event = ?event);

                    // Consume events until delay amount of time has passed since the latest event.
                    while receiver.recv_timeout(delay).is_ok() {}

                    debug!(message = "Consumed file change events for delay.", delay = ?delay);

                    // We need to read paths to resolve any inode changes that may have happened.
                    // And we need to do it before raising sighup to avoid missing any change.
                    if let Err(error) = watcher.add_paths(&config_paths) {
                        error!(message = "Failed to read files to watch.", %error);
                        break;
                    }

                    debug!(message = "Reloaded paths.");

                    info!("Configuration file changed.");
                    _ = signal_tx.send(crate::signal::SignalTo::ReloadFromDisk).map_err(|error| {
                        error!(message = "Unable to reload configuration file. Restart Vector to reload it.", cause = %error)
                    });
                } else {
                    debug!(message = "Ignoring event.", event = ?event)
                }
            }
        }

        thread::sleep(RETRY_TIMEOUT);

        watcher = create_watcher(&watcher_conf, &config_paths)
            .map_err(|error| error!(message = "Failed to create file watcher.", %error))
            .ok();

        if watcher.is_some() {
            // Config files could have changed while we weren't watching,
            // so for a good measure raise SIGHUP and let reload logic
            // determine if anything changed.
            info!("Speculating that configuration files have changed.");
            _ = signal_tx.send(crate::signal::SignalTo::ReloadFromDisk).map_err(|error| {
                error!(message = "Unable to reload configuration file. Restart Vector to reload it.", cause = %error)
            });
        }
    });

    Ok(())
}

fn create_watcher(
    watcher_conf: &WatcherConfig,
    config_paths: &[PathBuf],
) -> Result<(Watcher, Receiver<Result<notify::Event, notify::Error>>), Error> {
    info!("Creating configuration file watcher.");

    let (sender, receiver) = channel();
    let mut watcher = match watcher_conf {
        WatcherConfig::RecommendedWatcher => {
            let recommended_watcher = recommended_watcher(sender)?;
            Watcher::RecommendedWatcher(recommended_watcher)
        }
        WatcherConfig::PollWatcher(interval) => {
            let config =
                notify::Config::default().with_poll_interval(Duration::from_secs(*interval));
            let poll_watcher = notify::PollWatcher::new(sender, config)?;
            Watcher::PollWatcher(poll_watcher)
        }
    };
    watcher.add_paths(config_paths)?;
    Ok((watcher, receiver))
}

#[cfg(all(test, unix, not(target_os = "macos")))] // https://github.com/vectordotdev/vector/issues/5000
mod tests {
    use super::*;
    use crate::{
        signal::SignalRx,
        test_util::{temp_dir, temp_file, trace_init},
    };
    use std::{fs::File, io::Write, time::Duration};
    use tokio::sync::broadcast;

    async fn test(file: &mut File, timeout: Duration, mut receiver: SignalRx) -> bool {
        file.write_all(&[0]).unwrap();
        file.sync_all().unwrap();

        matches!(
            tokio::time::timeout(timeout, receiver.recv()).await,
            Ok(Ok(crate::signal::SignalTo::ReloadFromDisk))
        )
    }

    #[tokio::test]
    async fn file_directory_update() {
        trace_init();

        let delay = Duration::from_secs(3);
        let dir = temp_dir().to_path_buf();
        let file_path = dir.join("vector.toml");
        let watcher_conf = WatcherConfig::RecommendedWatcher;

        std::fs::create_dir(&dir).unwrap();
        let mut file = File::create(&file_path).unwrap();

        let (signal_tx, signal_rx) = broadcast::channel(128);
        spawn_thread(watcher_conf, signal_tx, &[dir], delay).unwrap();

        if !test(&mut file, delay * 5, signal_rx).await {
            panic!("Test timed out");
        }
    }

    #[tokio::test]
    async fn file_update() {
        trace_init();

        let delay = Duration::from_secs(3);
        let file_path = temp_file();
        let mut file = File::create(&file_path).unwrap();
        let watcher_conf = WatcherConfig::RecommendedWatcher;

        let (signal_tx, signal_rx) = broadcast::channel(128);
        spawn_thread(watcher_conf, signal_tx, &[file_path], delay).unwrap();

        if !test(&mut file, delay * 5, signal_rx).await {
            panic!("Test timed out");
        }
    }

    #[tokio::test]
    #[cfg(unix)]
    async fn sym_file_update() {
        trace_init();

        let delay = Duration::from_secs(3);
        let file_path = temp_file();
        let sym_file = temp_file();
        let mut file = File::create(&file_path).unwrap();
        std::os::unix::fs::symlink(&file_path, &sym_file).unwrap();

        let watcher_conf = WatcherConfig::RecommendedWatcher;

        let (signal_tx, signal_rx) = broadcast::channel(128);
        spawn_thread(watcher_conf, signal_tx, &[sym_file], delay).unwrap();

        if !test(&mut file, delay * 5, signal_rx).await {
            panic!("Test timed out");
        }
    }

    #[tokio::test]
    async fn recursive_directory_file_update() {
        trace_init();

        let delay = Duration::from_secs(3);
        let dir = temp_dir().to_path_buf();
        let sub_dir = dir.join("sources");
        let file_path = sub_dir.join("input.toml");
        let watcher_conf = WatcherConfig::RecommendedWatcher;

        std::fs::create_dir_all(&sub_dir).unwrap();
        let mut file = File::create(&file_path).unwrap();

        let (signal_tx, signal_rx) = broadcast::channel(128);
        spawn_thread(watcher_conf, signal_tx, &[sub_dir], delay).unwrap();

        if !test(&mut file, delay * 5, signal_rx).await {
            panic!("Test timed out");
        }
    }
}

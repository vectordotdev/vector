use std::{path::PathBuf, time::Duration};
#[cfg(unix)]
use std::{
    sync::mpsc::{channel, Receiver},
    thread,
};

#[cfg(unix)]
use notify::{recommended_watcher, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::Error;

/// Per notify own documentation, it's advised to have delay of more than 30 sec,
/// so to avoid receiving repetitions of previous events on macOS.
///
/// But, config and topology reload logic can handle:
///  - Invalid config, caused either by user or by data race.
///  - Frequent changes, caused by user/editor modifying/saving file in small chunks.
///    so we can use smaller, more responsive delay.
#[cfg(unix)]
const CONFIG_WATCH_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

#[cfg(unix)]
const RETRY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Triggers SIGHUP when file on config_path changes.
/// Accumulates file changes until no change for given duration has occurred.
/// Has best effort guarantee of detecting all file changes from the end of
/// this function until the main thread stops.
#[cfg(unix)]
pub fn spawn_thread<'a>(
    config_paths: impl IntoIterator<Item = &'a PathBuf> + 'a,
    delay: impl Into<Option<Duration>>,
) -> Result<(), Error> {
    let config_paths: Vec<_> = config_paths.into_iter().cloned().collect();
    let delay = delay.into().unwrap_or(CONFIG_WATCH_DELAY);

    // Create watcher now so not to miss any changes happening between
    // returning from this function and the thread starting.
    let mut watcher = Some(create_watcher(&config_paths)?);

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
                    if let Err(error) = add_paths(&mut watcher, &config_paths) {
                        error!(message = "Failed to read files to watch.", %error);
                        break;
                    }

                    debug!(message = "Reloaded paths.");

                    info!("Configuration file changed.");
                    raise_sighup();
                } else {
                    debug!(message = "Ignoring event.", event = ?event)
                }
            }
        }

        thread::sleep(RETRY_TIMEOUT);

        watcher = create_watcher(&config_paths)
            .map_err(|error| error!(message = "Failed to create file watcher.", %error))
            .ok();

        if watcher.is_some() {
            // Config files could have changed while we weren't watching,
            // so for a good measure raise SIGHUP and let reload logic
            // determine if anything changed.
            info!("Speculating that configuration files have changed.");
            raise_sighup();
        }
    });

    Ok(())
}

#[cfg(windows)]
/// Errors on Windows.
pub fn spawn_thread<'a>(
    _config_paths: impl IntoIterator<Item = &'a PathBuf> + 'a,
    _delay: impl Into<Option<Duration>>,
) -> Result<(), Error> {
    Err("Reloading config on Windows isn't currently supported. Related issue https://github.com/vectordotdev/vector/issues/938 .".into())
}

#[cfg(unix)]
fn raise_sighup() {
    use nix::sys::signal;
    _ = signal::raise(signal::Signal::SIGHUP).map_err(|error| {
        error!(message = "Unable to reload configuration file. Restart Vector to reload it.", cause = %error)
    });
}

#[cfg(unix)]
fn create_watcher(
    config_paths: &[PathBuf],
) -> Result<
    (
        RecommendedWatcher,
        Receiver<Result<notify::Event, notify::Error>>,
    ),
    Error,
> {
    info!("Creating configuration file watcher.");
    let (sender, receiver) = channel();
    let mut watcher = recommended_watcher(sender)?;
    add_paths(&mut watcher, config_paths)?;
    Ok((watcher, receiver))
}

#[cfg(unix)]
fn add_paths(watcher: &mut RecommendedWatcher, config_paths: &[PathBuf]) -> Result<(), Error> {
    for path in config_paths {
        watcher.watch(path, RecursiveMode::Recursive)?;
    }
    Ok(())
}

#[cfg(all(test, unix, not(target_os = "macos")))] // https://github.com/vectordotdev/vector/issues/5000
mod tests {
    use std::{fs::File, io::Write, time::Duration};

    use tokio::signal::unix::{signal, SignalKind};

    use super::*;
    use crate::test_util::{temp_dir, temp_file, trace_init};

    async fn test(file: &mut File, timeout: Duration) -> bool {
        let mut signal = signal(SignalKind::hangup()).expect("Signal handlers should not panic.");

        file.write_all(&[0]).unwrap();
        file.sync_all().unwrap();

        tokio::time::timeout(timeout, signal.recv()).await.is_ok()
    }

    #[tokio::test]
    async fn file_directory_update() {
        trace_init();

        let delay = Duration::from_secs(3);
        let dir = temp_dir().to_path_buf();
        let file_path = dir.join("vector.toml");

        std::fs::create_dir(&dir).unwrap();
        let mut file = File::create(&file_path).unwrap();

        spawn_thread(&[dir], delay).unwrap();

        if !test(&mut file, delay * 5).await {
            panic!("Test timed out");
        }
    }

    #[tokio::test]
    async fn file_update() {
        trace_init();

        let delay = Duration::from_secs(3);
        let file_path = temp_file();
        let mut file = File::create(&file_path).unwrap();

        spawn_thread(&[file_path], delay).unwrap();

        if !test(&mut file, delay * 5).await {
            panic!("Test timed out");
        }
    }

    #[tokio::test]
    async fn sym_file_update() {
        trace_init();

        let delay = Duration::from_secs(3);
        let file_path = temp_file();
        let sym_file = temp_file();
        let mut file = File::create(&file_path).unwrap();
        std::os::unix::fs::symlink(&file_path, &sym_file).unwrap();

        spawn_thread(&[sym_file], delay).unwrap();

        if !test(&mut file, delay * 5).await {
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

        std::fs::create_dir_all(&sub_dir).unwrap();
        let mut file = File::create(&file_path).unwrap();

        spawn_thread(&[sub_dir], delay).unwrap();

        if !test(&mut file, delay * 5).await {
            panic!("Test timed out");
        }
    }
}

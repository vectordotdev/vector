use super::Config;
use crate::Error;
use notify::{watcher, DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver},
    thread,
    time::Duration,
};

/// Per notify own documentation, it's advised to have delay of more than 30 sec,
/// so to avoid receiving repetitions of previous events on macOS.
///
/// But, config and topology reload logic can handle:
///  - Invalid config, caused either by user or by data race.
///  - Frequent changes, caused by user/editor modifying/saving file in small chunks.
/// so we can use smaller, more responsive delay.
pub const CONFIG_WATCH_DELAY: std::time::Duration = std::time::Duration::from_secs(1);

const RETRY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// On unix triggers SIGHUP when file on config_path changes.
/// Accumulates file changes until no change for given duration has occured.
/// Has best effort guarante of detecting all file changes from the end of
/// this function until the main thread stops.
///
/// Errors on Windows.
pub fn config_watcher(config: &Config, config_path: PathBuf, delay: Duration) -> Result<(), Error> {
    if config.global.reload_config {
        #[cfg(unix)]
        {
            // Create watcher now so not to miss any changes happening between
            // returning from this function and thread started.
            let mut watcher = create_watcher(&config_path, delay);

            thread::spawn(move || loop {
                if let Some((_, receiver)) = watcher.take() {
                    info!("Watching configuration file.");
                    while let Ok(msg) = receiver.recv() {
                        match msg {
                            DebouncedEvent::Write(_)
                            | DebouncedEvent::Create(_)
                            | DebouncedEvent::Remove(_) => {
                                info!("Configuration file changed.");
                                raise_sighup()
                            }
                            event => debug!(message = "Ignoring event", ?event),
                        }
                    }
                    error!("Stoped watching configuration file.");
                }

                thread::sleep(RETRY_TIMEOUT);

                watcher = create_watcher(&config_path, delay);

                if watcher.is_some() {
                    // Config file could have changed while we weren't watching,
                    // so for a good measure raise SIGHUP and let reload logic
                    // to determine if anything changed.
                    info!("Speculating that configuration file changed.");
                    raise_sighup();
                }
            });

            Ok(())
        }

        #[cfg(windows)]
        {
            Err("Reloading config on Windows isn't currently supported. Related issue https://github.com/timberio/vector/issues/938 .")
        }
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn raise_sighup() {
    use nix::sys::signal;
    let _ = signal::raise(signal::Signal::SIGHUP).map_err(|error| {
        error!(message = "Unable to reload configuration file. Restart Vector to reload it.", cause = ?error)
    });
}

fn create_watcher(
    config_path: &Path,
    delay: Duration,
) -> Option<(RecommendedWatcher, Receiver<DebouncedEvent>)> {
    info!("Creating configuration file watcher");
    let (sender, receiver) = channel();
    match watcher(sender, delay) {
        Ok(mut watcher) => match watcher.watch(&config_path, RecursiveMode::NonRecursive) {
            Ok(_) => {
                return Some((watcher, receiver));
            }
            Err(error) => error!(message = "Failed to watch configuration file", ?error),
        },
        Err(error) => error!(message = "Failed to create file watcher", ?error),
    }
    None
}

#[cfg(test)]
mod tests {
    use super::Config;
    use super::*;
    use crate::test_util::{runtime, temp_file};
    use futures::future;
    use futures::{Future, Stream};
    use std::time::{Duration, Instant};
    use std::{fs::File, io::Write};
    use tokio::timer::Delay;
    use tokio_signal::unix::{Signal, SIGHUP};

    #[test]
    fn file_update() {
        crate::test_util::trace_init();
        let delay = Duration::from_secs(1);
        let file_path = temp_file();
        let mut file = File::create(&file_path).unwrap();

        let mut config = Config::empty();
        config.global.reload_config = true;

        config_watcher(&config, file_path, delay);

        file.write_all(&[0]).unwrap();
        std::mem::drop(file);

        let mut rt = runtime();

        let signal = Signal::new(SIGHUP).flatten_stream();
        let result = rt
            .block_on(
                signal
                    .into_future()
                    .select2(Delay::new(Instant::now() + delay * 5)),
            )
            .ok()
            .unwrap();

        match result {
            future::Either::A(_) => (), //OK
            future::Either::B(_) => panic!("Test timed out"),
        }
    }
}

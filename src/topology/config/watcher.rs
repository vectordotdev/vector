use super::Config;
use futures::{stream, sync::mpsc, Async, Stream};
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
/// Doesn't do anything on Windows.
pub fn config_watcher(
    config: &Config,
    config_path: PathBuf,
    duration: Duration,
) {
    #[cfg(unix)]
    if config.global.reload_config {
        // Create watcher now so not to miss any changes happening between
        // returning from this function and thread started.
        let mut watcher = create_watcher(&config_path, delay);

        thread::spawn(move || {
            loop {
                if let Some((_, receiver)) = watcher.take() {
                    info!("Watching configuration file");
                    while let Ok(msg) = receiver.recv() {
                        match msg {
                            DebouncedEvent::Write(_)
                            | DebouncedEvent::Create(_)
                            | DebouncedEvent::Remove(_) => {
                                info!("Configuration file changed");
                                nix::sys::signal::raise(nix::sys::signal::Signal::SIGHUP);
                            }
                            event => debug!(message = "Ignoring event", ?event),
                        }
                    }
                    error!("Stoped watching configuration file");
                }

                thread::sleep(RETRY_TIMEOUT);

                watcher = create_watcher(&config_path, delay);
            }
        });
    }
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

    #[test]
    fn file_update() {
        crate::test_util::trace_init();
        let delay = Duration::from_secs(1);
        let file_path = temp_file();
        let mut file = File::create(&file_path).unwrap();

        let mut config = Config::empty();
        config.global.reload_config = true;

        let mut watcher = config_watcher(&config, file_path, delay);

        file.write_all(&[0]).unwrap();
        std::mem::drop(file);

        let mut rt = runtime();

        let result = rt
            .block_on(
                future::poll_fn(move || watcher.poll())
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

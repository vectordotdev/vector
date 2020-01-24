use super::Config;
use futures::{stream, sync::mpsc, Async, Stream};
use notify::{watcher, DebouncedEvent, RecommendedWatcher, RecursiveMode, Watcher};
use std::{
    path::{Path, PathBuf},
    sync::mpsc::{channel, Receiver},
    thread,
    time::Duration,
};

// Per notify own documentation, it's advised to have delay of more than 30 sec,
// so to avoid receiving repetitions of previous events on macOS.
// Larger delays hurt responsivity, but that's fine as this is primarily designed
// for cloud usage which isn't exactly super responsive.
pub const CONFIG_WATCH_DELAY: std::time::Duration = std::time::Duration::from_secs(31);

const RETRY_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Returns when config should be reloaded.
/// Never errors, and never ends.
pub fn config_watcher(
    config: &Config,
    config_path: PathBuf,
    delay: Duration,
) -> impl Stream<Item = (), Error = ()> {
    // Each sender has one slot, and 0 shared slots.
    let (mut out_sender, out_receiver) = mpsc::channel(0);

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
                                // If Ok, good, if Err, then there is already a message
                                // in the channel so this one is not required.
                                let _ = out_sender.try_send(());
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

    let mut change_stream = out_receiver.fuse();
    stream::poll_fn(move || {
        change_stream.poll().map(|asyn| match asyn {
            // Fulfills neverending promise
            Async::Ready(None) => Async::NotReady,
            asyn => asyn,
        })
    })
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
        // Windows won't emit Write event while File handle is present.
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

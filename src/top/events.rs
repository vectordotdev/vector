use std::io;
use std::sync::mpsc;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::{thread, time::Duration};
use termion::{event::Key, input::TermRead};

pub enum Event<I> {
    Input(I),
    Tick,
}

/// A small event handler that wrap termion input and tick events. Each event
/// type is handled in its own thread and returned to a common `Receiver`
pub struct Events {
    rx: mpsc::Receiver<Event<Key>>,
    _input_handle: thread::JoinHandle<()>,
    _ignore_exit_key: Arc<AtomicBool>,
    _tick_handle: thread::JoinHandle<()>,
}

#[derive(Debug, Clone, Copy)]
pub struct Config {
    pub exit_key: Key,
    pub tick_rate: Duration,
}

impl Default for Config {
    fn default() -> Config {
        Config {
            exit_key: Key::Char('q'),
            tick_rate: Duration::from_millis(250),
        }
    }
}

impl Events {
    /// Creates a new keyboard events handler
    pub fn new() -> Events {
        Events::with_config(Config::default())
    }

    /// Creates a new Events with the provided configuration
    pub fn with_config(config: Config) -> Events {
        let (tx, rx) = mpsc::channel();
        let ignore_exit_key = Arc::new(AtomicBool::new(false));
        let _input_handle = {
            let tx = tx.clone();
            let ignore_exit_key = ignore_exit_key.clone();
            thread::spawn(move || {
                let stdin = io::stdin();
                for evt in stdin.keys() {
                    if let Ok(key) = evt {
                        if let Err(err) = tx.send(Event::Input(key)) {
                            eprintln!("{}", err);
                            return;
                        }
                        if !ignore_exit_key.load(Ordering::Relaxed) && key == config.exit_key {
                            return;
                        }
                    }
                }
            })
        };
        let _tick_handle = {
            thread::spawn(move || loop {
                if tx.send(Event::Tick).is_err() {
                    break;
                }
                thread::sleep(config.tick_rate);
            })
        };
        Events {
            rx,
            _ignore_exit_key: ignore_exit_key,
            _input_handle,
            _tick_handle,
        }
    }

    /// Returns the next keyboard event, when available
    pub fn next(&self) -> Result<Event<Key>, mpsc::RecvError> {
        self.rx.recv()
    }

    /// Disables the exit key. If the user navigates to a sub-component, disabling the
    /// exit key temporarily can make sense, to override the behavior of terminating the
    /// app entirely.
    /// https://github.com/timberio/vector/issues/4085
    pub fn _disable_exit_key(&mut self) {
        self._ignore_exit_key.store(true, Ordering::Relaxed);
    }

    /// Enables the exit key. This is useful if the exit key was previously disabled due
    /// to navigation to a sub-component, and 'exiting' really means 'going back'.
    /// Not currently in use; hence _
    pub fn _enable_exit_key(&mut self) {
        self._ignore_exit_key.store(false, Ordering::Relaxed);
    }
}

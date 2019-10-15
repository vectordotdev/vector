use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
    trace::{current_span, Instrument},
};
use bytes::{Bytes, BytesMut};
use file_source::{FileServer, Fingerprinter};
use futures::{future, sync::mpsc, Async, Future, Poll, Sink, Stream};
use regex::bytes::Regex;
use serde::{Deserialize, Serialize};
use snafu::{ResultExt, Snafu};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime};
use tokio::timer::DelayQueue;

#[derive(Debug, Snafu)]
enum BuildError {
    #[snafu(display("data_dir option required, but not given here or globally"))]
    NoDataDir,
    #[snafu(display(
        "could not create subdirectory {:?} inside of data_dir {:?}",
        subdir,
        data_dir
    ))]
    MakeSubdirectoryError {
        subdir: PathBuf,
        data_dir: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("data_dir {:?} does not exist", data_dir))]
    MissingDataDir { data_dir: PathBuf },
    #[snafu(display("data_dir {:?} is not writable", data_dir))]
    DataDirNotWritable { data_dir: PathBuf },
    #[snafu(display(
        "message_start_indicator {:?} is not a valid regex: {}",
        indicator,
        source
    ))]
    InvalidMessageStartIndicator {
        indicator: String,
        source: regex::Error,
    },
}

#[derive(Deserialize, Serialize, Debug, PartialEq)]
#[serde(deny_unknown_fields, default)]
pub struct FileConfig {
    pub include: Vec<PathBuf>,
    pub exclude: Vec<PathBuf>,
    pub file_key: Option<String>,
    pub start_at_beginning: bool,
    pub ignore_older: Option<u64>, // secs
    #[serde(default = "default_max_line_bytes")]
    pub max_line_bytes: usize,
    pub host_key: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub glob_minimum_cooldown: u64, // millis
    pub fingerprinting: FingerprintingConfig,
    pub message_start_indicator: Option<String>,
    pub multi_line_timeout: u64, // millis
    pub max_read_bytes: usize,
    pub oldest_first: bool,
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq)]
#[serde(tag = "strategy", rename_all = "snake_case")]
pub enum FingerprintingConfig {
    Checksum {
        fingerprint_bytes: usize,
        ignored_header_bytes: usize,
    },
    #[serde(rename = "device_and_inode")]
    DevInode,
}

impl From<FingerprintingConfig> for Fingerprinter {
    fn from(config: FingerprintingConfig) -> Fingerprinter {
        match config {
            FingerprintingConfig::Checksum {
                fingerprint_bytes,
                ignored_header_bytes,
            } => Fingerprinter::Checksum {
                fingerprint_bytes,
                ignored_header_bytes,
            },
            FingerprintingConfig::DevInode => Fingerprinter::DevInode,
        }
    }
}

fn default_max_line_bytes() -> usize {
    bytesize::kib(100u64) as usize
}

impl Default for FileConfig {
    fn default() -> Self {
        Self {
            include: vec![],
            exclude: vec![],
            file_key: Some("file".to_string()),
            start_at_beginning: false,
            ignore_older: None,
            max_line_bytes: default_max_line_bytes(),
            fingerprinting: FingerprintingConfig::Checksum {
                fingerprint_bytes: 256,
                ignored_header_bytes: 0,
            },
            host_key: None,
            data_dir: None,
            glob_minimum_cooldown: 1000, // millis
            message_start_indicator: None,
            multi_line_timeout: 1000, // millis
            max_read_bytes: 2048,
            oldest_first: false,
        }
    }
}

#[typetag::serde(name = "file")]
impl SourceConfig for FileConfig {
    fn build(
        &self,
        name: &str,
        globals: &GlobalOptions,
        out: mpsc::Sender<Event>,
    ) -> crate::Result<super::Source> {
        // add the source name as a subdir, so that multiple sources can
        // operate within the same given data_dir (e.g. the global one)
        // without the file servers' checkpointers interfering with each
        // other
        let data_dir = globals.resolve_and_make_data_subdir(self.data_dir.as_ref(), name)?;

        if let Some(ref indicator) = self.message_start_indicator {
            Regex::new(indicator).with_context(|| InvalidMessageStartIndicator { indicator })?;
        }

        Ok(file_source(self, data_dir, out))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

pub fn file_source(
    config: &FileConfig,
    data_dir: PathBuf,
    out: mpsc::Sender<Event>,
) -> super::Source {
    let (shutdown_tx, shutdown_rx) = std::sync::mpsc::channel();

    let ignore_before = config
        .ignore_older
        .map(|secs| SystemTime::now() - Duration::from_secs(secs));
    let glob_minimum_cooldown = Duration::from_millis(config.glob_minimum_cooldown);

    let file_server = FileServer {
        include: config.include.clone(),
        exclude: config.exclude.clone(),
        max_read_bytes: config.max_read_bytes,
        start_at_beginning: config.start_at_beginning,
        ignore_before,
        max_line_bytes: config.max_line_bytes,
        data_dir,
        glob_minimum_cooldown: glob_minimum_cooldown,
        fingerprinter: config.fingerprinting.clone().into(),
        oldest_first: config.oldest_first,
    };

    let file_key = config.file_key.clone();
    let host_key = config.host_key.clone().unwrap_or(event::HOST.to_string());
    let hostname = hostname::get_hostname();

    let include = config.include.clone();
    let exclude = config.exclude.clone();
    let message_start_indicator = config.message_start_indicator.clone();
    let multi_line_timeout = config.multi_line_timeout;
    Box::new(future::lazy(move || {
        info!(message = "Starting file server.", ?include, ?exclude);

        // sizing here is just a guess
        let (tx, rx) = futures::sync::mpsc::channel(100);

        let messages: Box<dyn Stream<Item = (Bytes, String), Error = ()> + Send> =
            if let Some(msi) = message_start_indicator {
                Box::new(LineAgg::new(
                    rx,
                    Regex::new(&msi).unwrap(), // validated in build
                    multi_line_timeout,
                ))
            } else {
                Box::new(rx)
            };

        let span = current_span();
        let span2 = span.clone();
        tokio::spawn(
            messages
                .map(move |(msg, file): (Bytes, String)| {
                    let _enter = span2.enter();
                    trace!(
                        message = "Received one event.",
                        file = file.as_str(),
                        rate_limit_secs = 10
                    );
                    create_event(msg, file, &host_key, &hostname, &file_key)
                })
                .forward(out.sink_map_err(|e| error!(%e)))
                .map(|_| ())
                .instrument(span),
        );

        let span = info_span!("file_server");
        thread::spawn(move || {
            let _enter = span.enter();
            file_server.run(tx.sink_map_err(drop), shutdown_rx);
        });

        // Dropping shutdown_tx is how we signal to the file server that it's time to shut down,
        // so it needs to be held onto until the future we return is dropped.
        future::empty().inspect(|_| drop(shutdown_tx))
    }))
}

struct LineAgg<T> {
    inner: T,
    marker: Regex,
    timeout: u64,
    buffers: HashMap<String, BytesMut>,
    draining: Option<Vec<(Bytes, String)>>,
    timeouts: DelayQueue<String>,
    expired: VecDeque<String>,
}

impl<T> LineAgg<T> {
    fn new(inner: T, marker: Regex, timeout: u64) -> Self {
        Self {
            inner,
            marker,
            timeout,
            draining: None,
            buffers: HashMap::new(),
            timeouts: DelayQueue::new(),
            expired: VecDeque::new(),
        }
    }
}

impl<T: Stream<Item = (Bytes, String), Error = ()>> Stream for LineAgg<T> {
    type Item = (Bytes, String);
    type Error = ();

    fn poll(&mut self) -> Poll<Option<Self::Item>, Self::Error> {
        loop {
            if let Some(to_drain) = &mut self.draining {
                if let Some((data, key)) = to_drain.pop() {
                    return Ok(Async::Ready(Some((data, key))));
                } else {
                    return Ok(Async::Ready(None));
                }
            }

            // check for keys that have hit their timeout
            while let Ok(Async::Ready(Some(expired_key))) = self.timeouts.poll() {
                self.expired.push_back(expired_key.into_inner());
            }

            match self.inner.poll() {
                Ok(Async::Ready(Some((line, src)))) => {
                    // look for buffered content from same source
                    if self.buffers.contains_key(&src) {
                        if self.marker.is_match(line.as_ref()) {
                            // buffer the incoming line and flush the existing data
                            let buffered = self
                                .buffers
                                .insert(src.clone(), line.into())
                                .expect("already asserted key is present");
                            return Ok(Async::Ready(Some((buffered.freeze(), src))));
                        } else {
                            // append new line to the buffered data
                            let buffered = self
                                .buffers
                                .get_mut(&src)
                                .expect("already asserted key is present");
                            buffered.extend_from_slice(b"\n");
                            buffered.extend_from_slice(&line);
                        }
                    } else {
                        // no existing data for this source so buffer it with timeout
                        self.timeouts
                            .insert(src.clone(), Duration::from_millis(self.timeout));
                        self.buffers.insert(src, line.into());
                    }
                }
                Ok(Async::Ready(None)) => {
                    // start flushing all existing data, stop polling inner
                    self.draining =
                        Some(self.buffers.drain().map(|(k, v)| (v.into(), k)).collect());
                }
                Ok(Async::NotReady) => {
                    if let Some(key) = self.expired.pop_front() {
                        if let Some(buffered) = self.buffers.remove(&key) {
                            return Ok(Async::Ready(Some((buffered.freeze(), key))));
                        }
                    }

                    return Ok(Async::NotReady);
                }
                Err(()) => return Err(()),
            };
        }
    }
}

fn create_event(
    line: Bytes,
    file: String,
    host_key: &String,
    hostname: &Option<String>,
    file_key: &Option<String>,
) -> Event {
    let mut event = Event::from(line);

    if let Some(file_key) = &file_key {
        event
            .as_mut_log()
            .insert_implicit(file_key.clone().into(), file.into());
    }

    if let Some(hostname) = &hostname {
        event
            .as_mut_log()
            .insert_implicit(host_key.clone().into(), hostname.clone().into());
    }

    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event;
    use crate::sources::file;
    use crate::test_util::{block_on, shutdown_on_idle};
    use crate::topology::Config;
    use futures::{Future, Stream};
    use std::collections::HashSet;
    use std::fs::{self, File};
    use std::io::{Seek, Write};
    use stream_cancel::Tripwire;
    use tempfile::tempdir;
    use tokio::util::FutureExt;

    fn test_default_file_config(dir: &tempfile::TempDir) -> file::FileConfig {
        file::FileConfig {
            fingerprinting: FingerprintingConfig::Checksum {
                fingerprint_bytes: 8,
                ignored_header_bytes: 0,
            },
            data_dir: Some(dir.path().to_path_buf()),
            glob_minimum_cooldown: 0, // millis
            ..Default::default()
        }
    }

    fn wait_with_timeout<F, R, E>(future: F) -> R
    where
        F: Send + 'static + Future<Item = R, Error = E>,
        R: Send + 'static,
        E: Send + 'static + std::fmt::Debug,
    {
        let result = block_on(future.timeout(Duration::from_secs(5)));
        assert!(
            result.is_ok(),
            "Unclosed channel: may indicate file-server could not shutdown gracefully."
        );
        result.unwrap()
    }

    fn sleep() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    #[test]
    fn parse_config() {
        let config: FileConfig = toml::from_str(
            r#"
        "#,
        )
        .unwrap();
        assert_eq!(config, FileConfig::default());
        assert_eq!(
            config.fingerprinting,
            FingerprintingConfig::Checksum {
                fingerprint_bytes: 256,
                ignored_header_bytes: 0,
            }
        );

        let config: FileConfig = toml::from_str(
            r#"
        [fingerprinting]
        strategy = "device_and_inode"
        "#,
        )
        .unwrap();
        assert_eq!(config.fingerprinting, FingerprintingConfig::DevInode);

        let config: FileConfig = toml::from_str(
            r#"
        [fingerprinting]
        strategy = "checksum"
        fingerprint_bytes = 128
        ignored_header_bytes = 512
        "#,
        )
        .unwrap();
        assert_eq!(
            config.fingerprinting,
            FingerprintingConfig::Checksum {
                fingerprint_bytes: 128,
                ignored_header_bytes: 512,
            }
        );
    }

    #[test]
    fn resolve_data_dir() {
        let global_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();

        let mut config = Config::empty();
        config.global.data_dir = global_dir.into_path().into();

        // local path given -- local should win
        let res = config
            .global
            .resolve_and_validate_data_dir(test_default_file_config(&local_dir).data_dir.as_ref())
            .unwrap();
        assert_eq!(res, local_dir.path());

        // no local path given -- global fallback should be in effect
        let res = config.global.resolve_and_validate_data_dir(None).unwrap();
        assert_eq!(res, config.global.data_dir.unwrap());
    }

    #[test]
    fn file_create_event() {
        let line = Bytes::from("hello world");
        let file = "some_file.rs".to_string();
        let host_key = "host".to_string();
        let hostname = Some("Some.Machine".to_string());
        let file_key = Some("file".to_string());

        let event = create_event(line, file, &host_key, &hostname, &file_key);
        let log = event.into_log();

        assert_eq!(log[&"file".into()], "some_file.rs".into());
        assert_eq!(log[&"host".into()], "Some.Machine".into());
        assert_eq!(log[&event::MESSAGE], "hello world".into());
    }

    #[test]
    fn file_happy_path() {
        let n = 5;
        let (tx, rx) = futures::sync::mpsc::channel(2 * n);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path1 = dir.path().join("file1");
        let path2 = dir.path().join("file2");
        let mut file1 = File::create(&path1).unwrap();
        let mut file2 = File::create(&path2).unwrap();

        sleep(); // The files must be observed at their original lengths before writing to them

        for i in 0..n {
            writeln!(&mut file1, "hello {}", i).unwrap();
            writeln!(&mut file2, "goodbye {}", i).unwrap();
        }

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(rx.collect());

        let mut hello_i = 0;
        let mut goodbye_i = 0;

        for event in received {
            let line = event.as_log()[&event::MESSAGE].to_string_lossy();
            if line.starts_with("hello") {
                assert_eq!(line, format!("hello {}", hello_i));
                assert_eq!(
                    event.as_log()[&"file".into()].to_string_lossy(),
                    path1.to_str().unwrap()
                );
                hello_i += 1;
            } else {
                assert_eq!(line, format!("goodbye {}", goodbye_i));
                assert_eq!(
                    event.as_log()[&"file".into()].to_string_lossy(),
                    path2.to_str().unwrap()
                );
                goodbye_i += 1;
            }
        }
        assert_eq!(hello_i, n);
        assert_eq!(goodbye_i, n);
    }

    #[test]
    fn file_truncate() {
        let n = 5;
        let (tx, rx) = futures::sync::mpsc::channel(2 * n);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };
        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        sleep(); // The files must be observed at its original length before writing to it

        for i in 0..n {
            writeln!(&mut file, "pretrunc {}", i).unwrap();
        }

        sleep(); // The writes must be observed before truncating

        file.set_len(0).unwrap();
        file.seek(std::io::SeekFrom::Start(0)).unwrap();

        sleep(); // The truncate must be observed before writing again

        for i in 0..n {
            writeln!(&mut file, "posttrunc {}", i).unwrap();
        }

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(rx.collect());

        let mut i = 0;
        let mut pre_trunc = true;

        for event in received {
            assert_eq!(
                event.as_log()[&"file".into()].to_string_lossy(),
                path.to_str().unwrap()
            );

            let line = event.as_log()[&event::MESSAGE].to_string_lossy();

            if pre_trunc {
                assert_eq!(line, format!("pretrunc {}", i));
            } else {
                assert_eq!(line, format!("posttrunc {}", i));
            }

            i += 1;
            if i == n {
                i = 0;
                pre_trunc = false;
            }
        }
    }

    #[test]
    fn file_rotate() {
        let n = 5;
        let (tx, rx) = futures::sync::mpsc::channel(2 * n);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };
        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let archive_path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        sleep(); // The files must be observed at its original length before writing to it

        for i in 0..n {
            writeln!(&mut file, "prerot {}", i).unwrap();
        }

        sleep(); // The writes must be observed before rotating

        fs::rename(&path, archive_path).unwrap();
        let mut file = File::create(&path).unwrap();

        sleep(); // The rotation must be observed before writing again

        for i in 0..n {
            writeln!(&mut file, "postrot {}", i).unwrap();
        }

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(rx.collect());

        let mut i = 0;
        let mut pre_rot = true;

        for event in received {
            assert_eq!(
                event.as_log()[&"file".into()].to_string_lossy(),
                path.to_str().unwrap()
            );

            let line = event.as_log()[&event::MESSAGE].to_string_lossy();

            if pre_rot {
                assert_eq!(line, format!("prerot {}", i));
            } else {
                assert_eq!(line, format!("postrot {}", i));
            }

            i += 1;
            if i == n {
                i = 0;
                pre_rot = false;
            }
        }
    }

    #[test]
    fn file_multiple_paths() {
        let n = 5;
        let (tx, rx) = futures::sync::mpsc::channel(4 * n);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*.txt"), dir.path().join("a.*")],
            exclude: vec![dir.path().join("a.*.txt")],
            ..test_default_file_config(&dir)
        };

        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path1 = dir.path().join("a.txt");
        let path2 = dir.path().join("b.txt");
        let path3 = dir.path().join("a.log");
        let path4 = dir.path().join("a.ignore.txt");
        let mut file1 = File::create(&path1).unwrap();
        let mut file2 = File::create(&path2).unwrap();
        let mut file3 = File::create(&path3).unwrap();
        let mut file4 = File::create(&path4).unwrap();

        sleep(); // The files must be observed at their original lengths before writing to them

        for i in 0..n {
            writeln!(&mut file1, "1 {}", i).unwrap();
            writeln!(&mut file2, "2 {}", i).unwrap();
            writeln!(&mut file3, "3 {}", i).unwrap();
            writeln!(&mut file4, "4 {}", i).unwrap();
        }

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(rx.collect());

        let mut is = [0; 3];

        for event in received {
            let line = event.as_log()[&event::MESSAGE].to_string_lossy();
            let mut split = line.split(" ");
            let file = split.next().unwrap().parse::<usize>().unwrap();
            assert_ne!(file, 4);
            let i = split.next().unwrap().parse::<usize>().unwrap();

            assert_eq!(is[file - 1], i);
            is[file - 1] += 1;
        }

        assert_eq!(is, [n as usize; 3]);
    }

    #[test]
    fn file_file_key() {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        let (trigger, tripwire) = Tripwire::new();

        // Default
        {
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                ..test_default_file_config(&dir)
            };

            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

            rt.spawn(source.select(tripwire.clone()).map(|_| ()).map_err(|_| ()));

            let path = dir.path().join("file");
            let mut file = File::create(&path).unwrap();

            sleep();

            writeln!(&mut file, "hello there").unwrap();

            sleep();

            let received = wait_with_timeout(rx.into_future()).0.unwrap();
            assert_eq!(
                received.as_log()[&"file".into()].to_string_lossy(),
                path.to_str().unwrap()
            );
        }

        // Custom
        {
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                file_key: Some("source".to_string()),
                ..test_default_file_config(&dir)
            };

            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

            rt.spawn(source.select(tripwire.clone()).map(|_| ()).map_err(|_| ()));

            let path = dir.path().join("file");
            let mut file = File::create(&path).unwrap();

            sleep();

            writeln!(&mut file, "hello there").unwrap();

            sleep();

            let received = wait_with_timeout(rx.into_future()).0.unwrap();
            assert_eq!(
                received.as_log()[&"source".into()].to_string_lossy(),
                path.to_str().unwrap()
            );
        }

        // Hidden
        {
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let dir = tempdir().unwrap();
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                file_key: None,
                ..test_default_file_config(&dir)
            };

            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

            rt.spawn(source.select(tripwire.clone()).map(|_| ()).map_err(|_| ()));

            let path = dir.path().join("file");
            let mut file = File::create(&path).unwrap();

            sleep();

            writeln!(&mut file, "hello there").unwrap();

            sleep();

            let received = wait_with_timeout(rx.into_future()).0.unwrap();
            assert_eq!(
                received.as_log().keys().cloned().collect::<HashSet<_>>(),
                vec![
                    event::HOST.clone(),
                    event::MESSAGE.clone(),
                    event::TIMESTAMP.clone()
                ]
                .into_iter()
                .collect::<HashSet<_>>()
            );
        }

        drop(trigger);
        shutdown_on_idle(rt);
    }

    #[test]
    fn file_start_position_server_restart() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();
        writeln!(&mut file, "zeroth line").unwrap();
        sleep();

        // First time server runs it picks up existing lines.
        {
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            let (trigger, tripwire) = Tripwire::new();
            rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

            sleep();
            writeln!(&mut file, "first line").unwrap();
            sleep();

            drop(trigger);
            shutdown_on_idle(rt);

            let received = wait_with_timeout(rx.collect());
            let lines = received
                .into_iter()
                .map(|event| event.as_log()[&event::MESSAGE].to_string_lossy())
                .collect::<Vec<_>>();
            assert_eq!(lines, vec!["zeroth line", "first line"]);
        }
        // Restart server, read file from checkpoint.
        {
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            let (trigger, tripwire) = Tripwire::new();
            rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

            sleep();
            writeln!(&mut file, "second line").unwrap();
            sleep();

            drop(trigger);
            shutdown_on_idle(rt);

            let received = wait_with_timeout(rx.collect());
            let lines = received
                .into_iter()
                .map(|event| event.as_log()[&event::MESSAGE].to_string_lossy())
                .collect::<Vec<_>>();
            assert_eq!(lines, vec!["second line"]);
        }
        // Restart server, read files from beginning.
        {
            let config = file::FileConfig {
                include: vec![dir.path().join("*")],
                start_at_beginning: true,
                ..test_default_file_config(&dir)
            };
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            let (trigger, tripwire) = Tripwire::new();
            rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

            sleep();
            writeln!(&mut file, "third line").unwrap();
            sleep();

            drop(trigger);
            shutdown_on_idle(rt);

            let received = wait_with_timeout(rx.collect());
            let lines = received
                .into_iter()
                .map(|event| event.as_log()[&event::MESSAGE].to_string_lossy())
                .collect::<Vec<_>>();
            assert_eq!(
                lines,
                vec!["zeroth line", "first line", "second line", "third line"]
            );
        }
    }
    #[test]
    fn file_start_position_server_restart_with_file_rotation() {
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..test_default_file_config(&dir)
        };

        let path = dir.path().join("file");
        let path_for_old_file = dir.path().join("file.old");
        // Run server first time, collect some lines.
        {
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            let (trigger, tripwire) = Tripwire::new();
            rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

            let mut file = File::create(&path).unwrap();
            sleep();
            writeln!(&mut file, "first line").unwrap();
            sleep();

            drop(trigger);
            shutdown_on_idle(rt);

            let received = wait_with_timeout(rx.collect());
            let lines = received
                .into_iter()
                .map(|event| event.as_log()[&event::MESSAGE].to_string_lossy())
                .collect::<Vec<_>>();
            assert_eq!(lines, vec!["first line"]);
        }
        // Perform 'file rotation' to archive old lines.
        fs::rename(&path, &path_for_old_file).unwrap();
        // Restart the server and make sure it does not re-read the old file
        // even though it has a new name.
        {
            let (tx, rx) = futures::sync::mpsc::channel(10);
            let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            let (trigger, tripwire) = Tripwire::new();
            rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

            let mut file = File::create(&path).unwrap();
            sleep();
            writeln!(&mut file, "second line").unwrap();
            sleep();

            drop(trigger);
            shutdown_on_idle(rt);

            let received = wait_with_timeout(rx.collect());
            let lines = received
                .into_iter()
                .map(|event| event.as_log()[&event::MESSAGE].to_string_lossy())
                .collect::<Vec<_>>();
            assert_eq!(lines, vec!["second line"]);
        }
    }

    #[test]
    fn file_start_position_ignore_old_files() {
        use std::os::unix::io::AsRawFd;
        use std::time::{Duration, SystemTime};

        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            start_at_beginning: true,
            ignore_older: Some(1000),
            ..test_default_file_config(&dir)
        };

        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let before_path = dir.path().join("before");
        let mut before_file = File::create(&before_path).unwrap();
        let after_path = dir.path().join("after");
        let mut after_file = File::create(&after_path).unwrap();

        writeln!(&mut before_file, "first line").unwrap(); // first few bytes make up unique file fingerprint
        writeln!(&mut after_file, "_first line").unwrap(); //   and therefore need to be non-identical

        {
            // Set the modified times
            let before = SystemTime::now() - Duration::from_secs(1010);
            let after = SystemTime::now() - Duration::from_secs(990);

            let before_time = libc::timeval {
                tv_sec: before
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as _,
                tv_usec: 0,
            };
            let before_times = [before_time, before_time];

            let after_time = libc::timeval {
                tv_sec: after
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs() as _,
                tv_usec: 0,
            };
            let after_times = [after_time, after_time];

            unsafe {
                libc::futimes(before_file.as_raw_fd(), before_times.as_ptr());
                libc::futimes(after_file.as_raw_fd(), after_times.as_ptr());
            }
        }

        sleep();
        writeln!(&mut before_file, "second line").unwrap();
        writeln!(&mut after_file, "_second line").unwrap();

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(rx.collect());
        let before_lines = received
            .iter()
            .filter(|event| {
                event.as_log()[&"file".into()]
                    .to_string_lossy()
                    .ends_with("before")
            })
            .map(|event| event.as_log()[&event::MESSAGE].to_string_lossy())
            .collect::<Vec<_>>();
        let after_lines = received
            .iter()
            .filter(|event| {
                event.as_log()[&"file".into()]
                    .to_string_lossy()
                    .ends_with("after")
            })
            .map(|event| event.as_log()[&event::MESSAGE].to_string_lossy())
            .collect::<Vec<_>>();
        assert_eq!(before_lines, vec!["second line"]);
        assert_eq!(after_lines, vec!["_first line", "_second line"]);
    }

    #[test]
    fn file_max_line_bytes() {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            max_line_bytes: 10,
            ..test_default_file_config(&dir)
        };

        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        sleep(); // The files must be observed at their original lengths before writing to them

        writeln!(&mut file, "short").unwrap();
        writeln!(&mut file, "this is too long").unwrap();
        writeln!(&mut file, "11 eleven11").unwrap();
        let super_long = std::iter::repeat("This line is super long and will take up more space that BufReader's internal buffer, just to make sure that everything works properly when multiple read calls are involved").take(10000).collect::<String>();
        writeln!(&mut file, "{}", super_long).unwrap();
        writeln!(&mut file, "exactly 10").unwrap();
        writeln!(&mut file, "it can end on a line that's too long").unwrap();

        sleep();
        sleep();

        writeln!(&mut file, "and then continue").unwrap();
        writeln!(&mut file, "last short").unwrap();

        sleep();
        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(
            rx.map(|event| event.as_log().get(&event::MESSAGE).unwrap().clone())
                .collect(),
        );

        assert_eq!(
            received,
            vec!["short".into(), "exactly 10".into(), "last short".into()]
        );
    }

    #[test]
    fn test_multi_line_aggregation() {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            message_start_indicator: Some("INFO".into()),
            multi_line_timeout: 25, // less than 50 in sleep()
            ..test_default_file_config(&dir)
        };

        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);

        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        sleep(); // The files must be observed at their original lengths before writing to them

        writeln!(&mut file, "leftover foo").unwrap();
        writeln!(&mut file, "INFO hello").unwrap();
        writeln!(&mut file, "INFO goodbye").unwrap();
        writeln!(&mut file, "part of goodbye").unwrap();

        sleep();

        writeln!(&mut file, "INFO hi again").unwrap();
        writeln!(&mut file, "and some more").unwrap();
        writeln!(&mut file, "INFO hello").unwrap();

        sleep();

        writeln!(&mut file, "too slow").unwrap();
        writeln!(&mut file, "INFO doesn't have").unwrap();
        writeln!(&mut file, "to be INFO in").unwrap();
        writeln!(&mut file, "the middle").unwrap();

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(
            rx.map(|event| event.as_log().get(&event::MESSAGE).unwrap().clone())
                .collect(),
        );

        assert_eq!(
            received,
            vec![
                "leftover foo".into(),
                "INFO hello".into(),
                "INFO goodbye\npart of goodbye".into(),
                "INFO hi again\nand some more".into(),
                "INFO hello".into(),
                "too slow".into(),
                "INFO doesn't have".into(),
                "to be INFO in\nthe middle".into(),
            ]
        );
    }

    #[test]
    fn test_fair_reads() {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            start_at_beginning: true,
            max_read_bytes: 1,
            oldest_first: false,
            ..test_default_file_config(&dir)
        };

        let older_path = dir.path().join("z_older_file");
        let mut older = File::create(&older_path).unwrap();

        sleep();

        let newer_path = dir.path().join("a_newer_file");
        let mut newer = File::create(&newer_path).unwrap();

        writeln!(&mut older, "hello i am the old file").unwrap();
        writeln!(&mut older, "i have been around a while").unwrap();
        writeln!(&mut older, "you can read newer files at the same time").unwrap();

        writeln!(&mut newer, "and i am the new file").unwrap();
        writeln!(&mut newer, "this should be interleaved with the old one").unwrap();
        writeln!(&mut newer, "which is fine because we want fairness").unwrap();

        sleep();

        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(
            rx.map(|event| event.as_log().get(&event::MESSAGE).unwrap().clone())
                .collect(),
        );

        assert_eq!(
            received,
            vec![
                "hello i am the old file".into(),
                "and i am the new file".into(),
                "i have been around a while".into(),
                "this should be interleaved with the old one".into(),
                "you can read newer files at the same time".into(),
                "which is fine because we want fairness".into(),
            ]
        );
    }

    #[test]
    fn test_oldest_first() {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();

        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            start_at_beginning: true,
            max_read_bytes: 1,
            oldest_first: true,
            ..test_default_file_config(&dir)
        };

        let older_path = dir.path().join("z_older_file");
        let mut older = File::create(&older_path).unwrap();

        sleep();

        let newer_path = dir.path().join("a_newer_file");
        let mut newer = File::create(&newer_path).unwrap();

        writeln!(&mut older, "hello i am the old file").unwrap();
        writeln!(&mut older, "i have been around a while").unwrap();
        writeln!(&mut older, "you should definitely read all of me first").unwrap();

        writeln!(&mut newer, "i'm new").unwrap();
        writeln!(&mut newer, "hopefully you read all the old stuff first").unwrap();
        writeln!(&mut newer, "because otherwise i'm not going to make sense").unwrap();

        sleep();

        let source = file::file_source(&config, config.data_dir.clone().unwrap(), tx);
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        sleep();

        drop(trigger);
        shutdown_on_idle(rt);

        let received = wait_with_timeout(
            rx.map(|event| event.as_log().get(&event::MESSAGE).unwrap().clone())
                .collect(),
        );

        assert_eq!(
            received,
            vec![
                "hello i am the old file".into(),
                "i have been around a while".into(),
                "you should definitely read all of me first".into(),
                "i'm new".into(),
                "hopefully you read all the old stuff first".into(),
                "because otherwise i'm not going to make sense".into(),
            ]
        );
    }
}

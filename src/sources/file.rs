use crate::{
    event::{self, Event},
    topology::config::{DataType, GlobalOptions, SourceConfig},
};
use bytes::Bytes;
use file_source::FileServer;
use futures::{future, sync::mpsc, Future, Sink};
use serde::{Deserialize, Serialize};
use std::fs::DirBuilder;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, SystemTime};
use tracing::dispatcher;

#[derive(Deserialize, Serialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct FileConfig {
    pub include: Vec<PathBuf>,
    pub exclude: Vec<PathBuf>,
    pub file_key: Option<String>,
    pub start_at_beginning: bool,
    pub ignore_older: Option<u64>, // secs
    #[serde(default = "default_max_line_bytes")]
    pub max_line_bytes: usize,
    pub fingerprint_bytes: usize,
    pub ignored_header_bytes: usize,
    pub host_key: Option<String>,
    pub data_dir: Option<PathBuf>,
    pub glob_minimum_cooldown: u64, // millis
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
            fingerprint_bytes: 256,
            ignored_header_bytes: 0,
            host_key: None,
            data_dir: None,
            glob_minimum_cooldown: 1000, // millis
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
    ) -> Result<super::Source, String> {
        let mut data_dir = resolve_and_validate_data_dir(&self, globals)?;
        // now before passing on the validated data_dir, we add the source_name as a subdir,
        // so that multiple sources can operate within the same given data_dir (e.g. the global one)
        // without the file servers' checkpointers interfering with each other
        data_dir.push(name);
        if let Err(e) = DirBuilder::new().create(&data_dir) {
            return Err(format!(
                "could not create subdirectory '{}' inside of data_dir '{}': {}",
                name,
                data_dir.parent().unwrap().display(),
                e
            ));
        };
        Ok(file_source(self, data_dir, out))
    }

    fn output_type(&self) -> DataType {
        DataType::Log
    }
}

fn resolve_and_validate_data_dir(
    config: &FileConfig,
    globals: &GlobalOptions,
) -> Result<PathBuf, String> {
    let data_dir = match config.data_dir.as_ref().or(globals.data_dir.as_ref()) {
        Some(v) => v.clone(),
        None => return Err("data_dir option required, but not given here or globally".into()),
    };
    if !data_dir.exists() {
        return Err(format!(
            "data_dir '{}' does not exist",
            data_dir.to_string_lossy()
        ));
    }
    let readonly = std::fs::metadata(&data_dir)
        .map(|meta| meta.permissions().readonly())
        .unwrap_or(true);
    if readonly {
        return Err(format!(
            "data_dir '{}' is not writable",
            data_dir.to_string_lossy()
        ));
    }
    Ok(data_dir)
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
        max_read_bytes: 2048,
        start_at_beginning: config.start_at_beginning,
        ignore_before,
        max_line_bytes: config.max_line_bytes,
        fingerprint_bytes: config.fingerprint_bytes,
        ignored_header_bytes: config.ignored_header_bytes,
        data_dir,
        glob_minimum_cooldown: glob_minimum_cooldown,
    };

    let file_key = config.file_key.clone();
    let host_key = config.host_key.clone().unwrap_or(event::HOST.to_string());
    let hostname = hostname::get_hostname();

    let out = out
        .sink_map_err(|_| ())
        .with(move |(line, file): (Bytes, String)| {
            trace!(message = "Received one event.", file = file.as_str());

            let event = create_event(line, file, &host_key, &hostname, &file_key);

            future::ok(event)
        });

    let include = config.include.clone();
    let exclude = config.exclude.clone();
    Box::new(future::lazy(move || {
        info!(message = "Starting file server.", ?include, ?exclude);

        let span = info_span!("file-server");
        let dispatcher = dispatcher::get_default(|d| d.clone());
        thread::spawn(move || {
            let dispatcher = dispatcher;
            dispatcher::with_default(&dispatcher, || {
                span.in_scope(|| {
                    file_server.run(out, shutdown_rx);
                })
            });
        });

        // Dropping shutdown_tx is how we signal to the file server that it's time to shut down,
        // so it needs to be held onto until the future we return is dropped.
        future::empty().inspect(|_| drop(shutdown_tx))
    }))
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
            fingerprint_bytes: 8,
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

    #[test]
    fn resolve_data_dir() {
        let global_dir = tempdir().unwrap();
        let local_dir = tempdir().unwrap();

        let mut config = Config::empty();
        config.data_dir = global_dir.into_path().into();

        // local path given -- local should win
        let res = super::resolve_and_validate_data_dir(
            &test_default_file_config(&local_dir),
            &GlobalOptions::from(&config),
        )
        .unwrap();
        assert_eq!(res, local_dir.path());

        // no local path given -- global fallback should be in effect
        let res = super::resolve_and_validate_data_dir(
            &Default::default(),
            &GlobalOptions::from(&config),
        )
        .unwrap();
        assert_eq!(res, config.data_dir.unwrap());
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

    fn sleep() {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

}

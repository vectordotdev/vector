use futures::{Future, Stream};
use router::sources::file;
use router::test_util::shutdown_on_idle;
use std::fs::{self, File};
use std::io::{Seek, Write};
use stream_cancel::Tripwire;
use tempfile::tempdir;

#[test]
fn happy_path() {
    let (tx, rx) = futures::sync::mpsc::channel(10);
    let (trigger, tripwire) = Tripwire::new();

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..Default::default()
    };

    let source = file::file_source(&config, tx);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

    let path1 = dir.path().join("file1");
    let path2 = dir.path().join("file2");
    let n = 5;
    let mut file1 = File::create(&path1).unwrap();
    let mut file2 = File::create(&path2).unwrap();

    sleep(); // The files must be observed at their original lengths before writing to them

    for i in 0..n {
        writeln!(&mut file1, "hello {}", i).unwrap();
        writeln!(&mut file2, "goodbye {}", i).unwrap();
    }

    let received = rx.take(n * 2).collect().wait().unwrap();
    drop(trigger);
    shutdown_on_idle(rt);

    let mut hello_i = 0;
    let mut goodbye_i = 0;

    for record in received {
        if record.line.starts_with("hello") {
            assert_eq!(record.line, format!("hello {}", hello_i));
            assert_eq!(record.custom[&"file".into()], path1.to_str().unwrap());
            hello_i += 1;
        } else {
            assert_eq!(record.line, format!("goodbye {}", goodbye_i));
            assert_eq!(record.custom[&"file".into()], path2.to_str().unwrap());
            goodbye_i += 1;
        }
    }
    assert_eq!(hello_i, n);
    assert_eq!(goodbye_i, n);
}

#[test]
fn truncate() {
    let (tx, rx) = futures::sync::mpsc::channel(10);
    let (trigger, tripwire) = Tripwire::new();

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..Default::default()
    };
    let source = file::file_source(&config, tx);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

    let path = dir.path().join("file");
    let n = 5;
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

    let received = rx.take(n * 2).collect().wait().unwrap();
    drop(trigger);
    shutdown_on_idle(rt);

    let mut i = 0;
    let mut pre_trunc = true;

    for record in received {
        assert_eq!(record.custom[&"file".into()], path.to_str().unwrap());
        if pre_trunc {
            assert_eq!(record.line, format!("pretrunc {}", i));
        } else {
            assert_eq!(record.line, format!("posttrunc {}", i));
        }

        i += 1;
        if i == n {
            i = 0;
            pre_trunc = false;
        }
    }
}

#[test]
fn rotate() {
    let (tx, rx) = futures::sync::mpsc::channel(10);
    let (trigger, tripwire) = Tripwire::new();

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*")],
        ..Default::default()
    };
    let source = file::file_source(&config, tx);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

    let path = dir.path().join("file");
    let archive_path = dir.path().join("file");
    let n = 5;
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

    let received = rx.take(n * 2).collect().wait().unwrap();
    drop(trigger);
    shutdown_on_idle(rt);

    let mut i = 0;
    let mut pre_rot = true;

    for record in received {
        assert_eq!(record.custom[&"file".into()], path.to_str().unwrap());
        if pre_rot {
            assert_eq!(record.line, format!("prerot {}", i));
        } else {
            assert_eq!(record.line, format!("postrot {}", i));
        }

        i += 1;
        if i == n {
            i = 0;
            pre_rot = false;
        }
    }
}

#[test]
fn multiple_paths() {
    let (tx, rx) = futures::sync::mpsc::channel(10);
    let (trigger, tripwire) = Tripwire::new();

    let dir = tempdir().unwrap();
    let config = file::FileConfig {
        include: vec![dir.path().join("*.txt"), dir.path().join("a.*")],
        exclude: vec![dir.path().join("a.*.txt")],
        ..Default::default()
    };

    let source = file::file_source(&config, tx);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

    let path1 = dir.path().join("a.txt");
    let path2 = dir.path().join("b.txt");
    let path3 = dir.path().join("a.log");
    let path4 = dir.path().join("a.ignore.txt");
    let n = 5;
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

    let received = rx.take(n * 3).collect().wait().unwrap();
    drop(trigger);
    shutdown_on_idle(rt);

    let mut is = [0; 3];

    for record in received {
        let mut split = record.line.split(" ");
        let file = split.next().unwrap().parse::<usize>().unwrap();
        assert_ne!(file, 4);
        let i = split.next().unwrap().parse::<usize>().unwrap();

        assert_eq!(is[file - 1], i);
        is[file - 1] += 1;
    }

    assert_eq!(is, [n as usize; 3]);
}

#[test]
fn context_key() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let (trigger, tripwire) = Tripwire::new();

    // Default
    {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..Default::default()
        };

        let source = file::file_source(&config, tx);

        rt.spawn(source.select(tripwire.clone()).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        sleep();

        writeln!(&mut file, "hello").unwrap();

        let received = rx.into_future().wait().unwrap().0.unwrap();
        assert_eq!(received.custom[&"file".into()], path.to_str().unwrap());
    }

    // Custom
    {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            context_key: Some("source".to_string()),
            ..Default::default()
        };

        let source = file::file_source(&config, tx);

        rt.spawn(source.select(tripwire.clone()).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        sleep();

        writeln!(&mut file, "hello").unwrap();

        let received = rx.into_future().wait().unwrap().0.unwrap();
        assert_eq!(received.custom[&"source".into()], path.to_str().unwrap());
    }

    // Hidden
    {
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            context_key: None,
            ..Default::default()
        };

        let source = file::file_source(&config, tx);

        rt.spawn(source.select(tripwire.clone()).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        sleep();

        writeln!(&mut file, "hello").unwrap();

        let received = rx.into_future().wait().unwrap().0.unwrap();
        assert!(received.custom.is_empty());
    }

    drop(trigger);
    shutdown_on_idle(rt);
}

#[test]
fn start_position() {
    // Default (start from end)
    {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            ..Default::default()
        };

        let source = file::file_source(&config, tx);

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        writeln!(&mut file, "first line").unwrap();
        sleep();
        writeln!(&mut file, "second line").unwrap();
        sleep();

        drop(trigger);
        let received = rx.collect().wait().unwrap();
        let lines = received.into_iter().map(|r| r.line).collect::<Vec<_>>();
        assert_eq!(lines, vec!["second line"]);
    }

    // Start from beginning
    {
        let mut rt = tokio::runtime::Runtime::new().unwrap();
        let (tx, rx) = futures::sync::mpsc::channel(10);
        let (trigger, tripwire) = Tripwire::new();
        let dir = tempdir().unwrap();
        let config = file::FileConfig {
            include: vec![dir.path().join("*")],
            start_at_beginning: true,
            ..Default::default()
        };

        let source = file::file_source(&config, tx);

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let path = dir.path().join("file");
        let mut file = File::create(&path).unwrap();

        writeln!(&mut file, "first line").unwrap();
        sleep();
        writeln!(&mut file, "second line").unwrap();

        sleep();

        drop(trigger);
        let received = rx.collect().wait().unwrap();
        let lines = received.into_iter().map(|r| r.line).collect::<Vec<_>>();
        assert_eq!(lines, vec!["first line", "second line"]);
    }

    // Start from beginning (but ignore old files)
    {
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
            ..Default::default()
        };

        let source = file::file_source(&config, tx);

        rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

        let after_path = dir.path().join("after");
        let mut after_file = File::create(&after_path).unwrap();
        let before_path = dir.path().join("before");
        let mut before_file = File::create(&before_path).unwrap();

        writeln!(&mut after_file, "first line").unwrap();
        writeln!(&mut before_file, "first line").unwrap();

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
        writeln!(&mut after_file, "second line").unwrap();
        writeln!(&mut before_file, "second line").unwrap();

        sleep();

        drop(trigger);
        let received = rx.collect().wait().unwrap();
        let before_lines = received
            .iter()
            .filter(|r| r.custom[&"file".into()].ends_with("before"))
            .map(|r| r.line.clone())
            .collect::<Vec<_>>();
        let after_lines = received
            .iter()
            .filter(|r| r.custom[&"file".into()].ends_with("after"))
            .map(|r| r.line.clone())
            .collect::<Vec<_>>();
        assert_eq!(before_lines, vec!["second line"]);
        assert_eq!(after_lines, vec!["first line", "second line"]);
    }
}

fn sleep() {
    std::thread::sleep(std::time::Duration::from_millis(50));
}

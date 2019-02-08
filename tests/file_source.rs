use futures::{Future, Stream};
use router::sources::file;
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
    rt.shutdown_on_idle().wait().unwrap();

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
    rt.shutdown_on_idle().wait().unwrap();

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
    rt.shutdown_on_idle().wait().unwrap();

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
    };

    let source = file::file_source(&config, tx);

    let mut rt = tokio::runtime::Runtime::new().unwrap();

    rt.spawn(source.select(tripwire).map(|_| ()).map_err(|_| ()));

    let path1 = dir.path().join("a.txt");
    let path2 = dir.path().join("b.txt");
    let path3 = dir.path().join("a.log");
    let n = 5;
    let mut file1 = File::create(&path1).unwrap();
    let mut file2 = File::create(&path2).unwrap();
    let mut file3 = File::create(&path3).unwrap();

    sleep(); // The files must be observed at their original lengths before writing to them

    for i in 0..n {
        writeln!(&mut file1, "1 {}", i).unwrap();
        writeln!(&mut file2, "2 {}", i).unwrap();
        writeln!(&mut file3, "3 {}", i).unwrap();
    }

    let received = rx.take(n * 3).collect().wait().unwrap();
    drop(trigger);
    rt.shutdown_on_idle().wait().unwrap();

    let mut is = [0; 3];

    for record in received {
        let mut split = record.line.split(" ");
        let file = split.next().unwrap().parse::<usize>().unwrap() - 1;
        let i = split.next().unwrap().parse::<usize>().unwrap();

        assert_eq!(is[file], i);
        is[file] += 1;
    }

    assert_eq!(is, [n as usize; 3]);
}

fn sleep() {
    std::thread::sleep(std::time::Duration::from_millis(20));
}

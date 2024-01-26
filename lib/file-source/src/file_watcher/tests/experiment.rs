#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;
use std::{fs, io::Write};

use bytes::Bytes;
use quickcheck::{QuickCheck, TestResult};

use crate::{
    file_watcher::{tests::*, FileWatcher},
    ReadFrom,
};

// Interpret all FWActions, including truncation
//
// In the presence of truncation we cannot accurately determine which writes
// will eventually be read. This is because of the presence of buffered
// reading in file_watcher which pulls an unknown amount from the underlying
// disk. This is _good_ in the sense that we reduce the total number of file
// system reads and potentially retain data that would otherwise be lost
// during a truncation but is bad on account of we cannot guarantee _which_
// writes are lost in the presence of truncation.
//
// What we can do, though, is drive our FWFile model and the SUT at the same
// time, recording the total number of reads/writes. The SUT reads should be
// bounded below by the model reads, bounded above by the writes.
fn experiment(actions: Vec<FileWatcherAction>) {
    let dir = tempfile::TempDir::new().expect("could not create tempdir");
    let path = dir.path().join("a_file.log");
    let mut fp = fs::File::create(&path).expect("could not create");
    let mut rotation_count = 0;
    let mut fw = FileWatcher::new(
        path.clone(),
        ReadFrom::Beginning,
        None,
        100_000,
        Bytes::from("\n"),
    )
    .expect("must be able to create");

    let mut writes = 0;
    let mut sut_reads = 0;
    let mut model_reads = 0;

    let mut fwfiles: Vec<FileWatcherFile> = vec![FileWatcherFile::new()];
    let mut read_index = 0;
    for action in actions.iter() {
        match *action {
            FileWatcherAction::DeleteFile => {
                _ = fs::remove_file(&path);
                #[cfg(not(windows))] // Windows will only remove after the file is closed.
                assert!(!path.exists());
                fwfiles[0].reset();
                break;
            }
            FileWatcherAction::TruncateFile => {
                fwfiles[0].truncate();
                fp = fs::OpenOptions::new()
                    .read(true)
                    .write(true)
                    .truncate(true)
                    .open(&path)
                    .expect("could not truncate");
                #[cfg(unix)]
                assert_eq!(fp.metadata().expect("could not get metadata").size(), 0);
                #[cfg(windows)]
                assert_eq!(
                    fp.metadata().expect("could not get metadata").file_size(),
                    0
                );
                assert!(path.exists());
            }
            FileWatcherAction::Pause(ps) => delay(ps),
            FileWatcherAction::Exit => break,
            FileWatcherAction::WriteLine(ref s) => {
                fwfiles[0].write_line(s);
                assert!(fp.write_all(s.as_bytes()).is_ok());
                assert!(fp.write_all(b"\n").is_ok());
                assert!(fp.flush().is_ok());
                writes += 1;
            }
            FileWatcherAction::RotateFile => {
                let mut new_path = path.clone();
                new_path.set_extension(format!("log.{}", rotation_count));
                rotation_count += 1;
                fs::rename(&path, &new_path).expect("could not rename");
                fp = fs::File::create(&path).expect("could not create");
                fwfiles.insert(0, FileWatcherFile::new());
                read_index += 1;
            }
            FileWatcherAction::Read => {
                let mut attempts = 10;
                while attempts > 0 {
                    match fw.read_line() {
                        Err(_) => {
                            unreachable!();
                        }
                        Ok(Some(line)) if line.bytes.is_empty() => {
                            attempts -= 1;
                            continue;
                        }
                        Ok(None) => {
                            attempts -= 1;
                            continue;
                        }
                        Ok(_) => {
                            sut_reads += 1;
                            let psv = fwfiles[read_index].read_line();
                            if psv.is_some() {
                                model_reads += 1;
                                break;
                            }
                            break;
                        }
                    }
                }
            }
        }
    }
    assert!(writes >= sut_reads);
    assert!(sut_reads >= model_reads);
}

#[test]
fn file_watcher_with_truncation() {
    fn inner(actions: Vec<FileWatcherAction>) -> TestResult {
        experiment(actions);
        TestResult::passed()
    }
    QuickCheck::new()
        .tests(10000)
        .max_tests(100000)
        .quickcheck(inner as fn(Vec<FileWatcherAction>) -> TestResult);
}

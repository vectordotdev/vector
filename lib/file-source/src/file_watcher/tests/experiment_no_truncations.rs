use std::{fs, io::Write};

use bytes::Bytes;
use quickcheck::{QuickCheck, TestResult};

use crate::{
    file_watcher::{tests::*, FileWatcher},
    ReadFrom,
};

// Interpret all FWActions, excluding truncation
//
// This interpretation is the happy case. When there are no truncations our
// model and SUT should agree exactly. To that end, we confirm that every
// read from SUT exactly matches the reads from the model.
fn experiment_no_truncations(actions: Vec<FileWatcherAction>) {
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
            FileWatcherAction::TruncateFile => {}
            FileWatcherAction::Pause(ps) => delay(ps),
            FileWatcherAction::Exit => break,
            FileWatcherAction::WriteLine(ref s) => {
                fwfiles[0].write_line(s);
                assert!(fp.write_all(s.as_bytes()).is_ok());
                assert!(fp.write_all(b"\n").is_ok());
                assert!(fp.flush().is_ok());
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
                            assert!(fwfiles[read_index].read_line().is_none());
                            continue;
                        }
                        Ok(None) => {
                            attempts -= 1;
                            assert!(fwfiles[read_index].read_line().is_none());
                            continue;
                        }
                        Ok(Some(line)) => {
                            let exp = fwfiles[read_index].read_line().expect("could not readline");
                            assert_eq!(exp.into_bytes(), line.bytes);
                            // assert_eq!(sz, buf.len() + 1);
                            break;
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn file_watcher_no_truncation() {
    fn inner(actions: Vec<FileWatcherAction>) -> TestResult {
        experiment_no_truncations(actions);
        TestResult::passed()
    }
    QuickCheck::new()
        .tests(10000)
        .max_tests(100000)
        .quickcheck(inner as fn(Vec<FileWatcherAction>) -> TestResult);
}

mod file_server;
mod file_watcher;

pub use self::file_server::{FileServer, FileServerConfig};

#[cfg(test)]
mod test {
    extern crate tempdir;

    use self::file_watcher::FileWatcher;
    use super::*;
    use crate::time;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use std::fs;
    use std::io::Write;
    use std::os::unix::fs::MetadataExt;
    use std::str;
    // Welcome.
    //
    // This suite of tests is structured as an interpreter of file system
    // actions. You'll find two interpreters here, `experiment` and
    // `experiment_no_truncations`. These differ in one key respect: the later
    // does not interpret the 'trunction' instruction.
    //
    // What do I mean by all this? Well, what we're trying to do is validate the
    // behaviour of the file_watcher in the presence of arbitrary file-system
    // actions. These actions we call `FWAction`.
    #[derive(Clone, Debug)]
    enum FWAction {
        WriteLine(String),
        RotateFile,
        DeleteFile,
        TruncateFile,
        Read,
        Pause(u32),
        Exit,
    }
    // WriteLine writes an arbitrary line of text -- plus newline -- RotateFile
    // rotates the file as a log rotator might etc etc. Our interpreter
    // functions take these instructions and apply them to the system under test
    // (SUT), being a file_watcher pointed at a certain directory on-disk. In
    // this way we can drive the behaviour of file_watcher. Validation requires
    // a model, which we scattered between the interpreters -- as the model
    // varies slightly in the presense of truncation vs. not -- and FWFile.
    struct FWFile {
        contents: Vec<u8>,
        read_idx: usize,
        previous_read_size: usize,
        reads_available: usize,
    }
    // FWFile mimics an actual Unix file, at least for our purposes here. The
    // operations available on FWFile have to do with reading and writing lines,
    // truncation and resets, which mimic a delete/create cycle on the file
    // system. The function `FWFile::read_line` is the most complex and you're
    // warmly encouraged to read the documentation present there.
    impl FWFile {
        pub fn new() -> FWFile {
            FWFile {
                contents: vec![],
                read_idx: 0,
                previous_read_size: 0,
                reads_available: 0,
            }
        }

        pub fn reset(&mut self) {
            self.contents.truncate(0);
            self.read_idx = 0;
            self.previous_read_size = 0;
            self.reads_available = 0;
        }

        pub fn truncate(&mut self) {
            self.reads_available = 0;
            self.contents.truncate(0);
        }

        pub fn write_line(&mut self, input: &str) {
            self.contents.extend_from_slice(input.as_bytes());
            self.contents.push(b'\n');
            self.reads_available += 1;
        }

        /// Read a line from storage, if a line is availabe to be read.
        pub fn read_line(&mut self) -> Option<String> {
            // FWFile mimics a unix file being read in a buffered fashion,
            // driven by file_watcher. We _have_ to keep on top of where the
            // reader's read index -- called read_idx -- is between reads and
            // the size of the file -- called previous_read_size -- in the event
            // of trunction.
            //
            // If we detect in file_watcher that a truncation has happened then
            // the buffered reader is seeked back to 0. This is performed in
            // like kind when we reset read_idx to 0, as in the following case
            // where there are no reads available.
            if self.contents.is_empty() && self.reads_available == 0 {
                self.read_idx = 0;
                self.previous_read_size = 0;
                return None;
            }
            // Now, the above is done only when nothing has been written to the
            // FWFile or the contents have been totally removed. The trickier
            // case is where there are maybe _some_ things to be read but the
            // read_idx might be mis-set owing to truncations.
            //
            // `read_line` is performed in a line-wise fashion. start_idx
            // and end_idx are pulled apart from one another to find the
            // start and end of the line, if there's a line to be found.
            let mut end_idx;
            let start_idx;
            // Here's where we do truncation detection. When our file has
            // shrunk, restart the search at zero index. If the file is the
            // same size -- implying that it's either not changed or was
            // truncated and then filled back in before a read could occurr
            // -- we return None. Else, start searching at the present
            // read_idx.
            let max = self.contents.len();
            if self.previous_read_size > max {
                self.read_idx = 0;
                start_idx = 0;
                end_idx = 0;
            } else if self.read_idx == max {
                return None;
            } else {
                start_idx = self.read_idx;
                end_idx = self.read_idx;
            }
            // Seek end_idx foward until we hit the newline character.
            while self.contents[end_idx] != b'\n' {
                end_idx += 1;
                if end_idx == max {
                    return None;
                }
            }
            // Produce the read string -- minus its newline character -- and
            // set the control variables appropriately.
            let ret = str::from_utf8(&self.contents[start_idx..end_idx]).unwrap();
            self.read_idx = end_idx + 1;
            self.reads_available -= 1;
            self.previous_read_size = max;
            // There's a trick here. What happens if we _only_ read a
            // newline character. Well, that'll happen when truncations
            // cause trimmed reads and the only remaining character in the
            // line is the newline. Womp womp
            if !ret.is_empty() {
                return Some(ret.to_string());
            } else {
                return None;
            }
        }
    }

    impl Arbitrary for FWAction {
        fn arbitrary<G>(g: &mut G) -> FWAction
        where
            G: Gen,
        {
            let i: usize = g.gen_range(0, 100);
            let ln_sz = g.gen_range(1, 32);
            let pause = g.gen_range(1, 3);
            match i {
                // These weights are more or less arbitrary. 'Pause' maybe
                // doesn't have a use but we keep it in place to allow for
                // variations in file-system flushes.
                0...50 => {
                    FWAction::WriteLine(g.gen_ascii_chars().take(ln_sz).collect())
                }
                51...69 => FWAction::Read,
                70...75 => FWAction::Pause(pause),
                76...85 => FWAction::RotateFile,
                86...90 => FWAction::TruncateFile,
                91...95 => FWAction::DeleteFile,
                _ => FWAction::Exit,
            }
        }
    }

    // file_watcher switches files whenever the inode / devno pair changes for
    // the path under inspection. This function replicates a check that
    // file_watcher does internally.
    fn file_id(fp: &fs::File) -> (u64, u64) {
        use std::os::unix::fs::MetadataExt;
        let metadata = fp.metadata().unwrap();
        let dev = metadata.dev();
        let ino = metadata.ino();
        (dev, ino)
    }

    // Interpret all FWActions, including truncation
    //
    // In the presence of truncation we cannot accurately determine which writes
    // will eventually be read. This is because of the presence of buffered
    // reading in file_watcher which pulls an unknown amount from the underlying
    // disk. This is _good_ in the sense that we reduce the total number of file
    // system reads and potentially retain data that would otherwise be lost
    // during a truncation but is bad on account of we cannot guarantee _which_
    // writes are lost in the presense of truncation.
    //
    // What we can do, though, is drive our FWFile model and the SUT at the same
    // time, recording the total number of reads/writes. The SUT reads should be
    // bounded below by the model reads, bounded above by the writes.
    fn experiment(actions: Vec<FWAction>) {
        let dir = tempdir::TempDir::new("file_watcher_qc").unwrap();
        let path = dir.path().join("a_file.log");
        let mut fp = fs::File::create(&path).expect("could not create");
        let mut fp_id = file_id(&fp);
        let mut fw = FileWatcher::new(&path).expect("must be able to create");

        let mut writes = 0;
        let mut sut_reads = 0;
        let mut model_reads = 0;

        let mut fwfiles: Vec<FWFile> = vec![];
        fwfiles.push(FWFile::new());
        let mut read_index = 0;
        for action in actions.iter() {
            match *action {
                FWAction::DeleteFile => {
                    let _ = fs::remove_file(&path);
                    assert!(!path.exists());
                    fwfiles[0].reset();
                    break;
                }
                FWAction::TruncateFile => {
                    fwfiles[0].truncate();
                    fp = fs::OpenOptions::new()
                        .read(true)
                        .write(true)
                        .truncate(true)
                        .open(&path)
                        .unwrap();
                    assert_eq!(fp.metadata().unwrap().size(), 0);
                    let cur_file_id = file_id(&fp);
                    assert_eq!(cur_file_id, fp_id);
                    assert!(path.exists());
                }
                FWAction::Pause(ps) => time::delay(ps),
                FWAction::Exit => break,
                FWAction::WriteLine(ref s) => {
                    fwfiles[0].write_line(s);
                    assert!(fp.write(s.as_bytes()).is_ok());
                    assert!(fp.write("\n".as_bytes()).is_ok());
                    assert!(fp.flush().is_ok());
                    writes += 1;
                }
                FWAction::RotateFile => {
                    let mut new_path = path.clone();
                    new_path.set_extension("log.1");
                    match fs::rename(&path, &new_path) {
                        Ok(_) => {}
                        Err(_) => {
                            assert!(false);
                        }
                    }
                    fp = fs::File::create(&path).expect("could not create");
                    fp_id = file_id(&fp);
                    fwfiles.insert(0, FWFile::new());
                    read_index += 1;
                }
                FWAction::Read => {
                    let mut buf = String::new();
                    let mut attempts = 10;
                    while attempts > 0 {
                        match fw.read_line(&mut buf) {
                            Err(_) => {
                                unreachable!();
                            }
                            Ok(0) => {
                                attempts -= 1;
                                read_index = 0;
                                continue;
                            }
                            Ok(_) => {
                                sut_reads += 1;
                                loop {
                                    let psv = fwfiles[read_index].read_line();
                                    if psv.is_none() {
                                        if read_index == 0 {
                                            break;
                                        }
                                        read_index = 0;
                                    } else {
                                        model_reads += 1;
                                        break;
                                    }
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

    // Interpret all FWActions, excluding truncation
    //
    // This interpretation is the happy case. When there are no truncations our
    // model and SUT should agree exactly. To that end, we confirm that every
    // read from SUT exactly matches the reads from the model.
    fn experiment_no_truncations(actions: Vec<FWAction>) {
        let dir = tempdir::TempDir::new("file_watcher_qc").unwrap();
        let path = dir.path().join("a_file.log");
        let mut fp = fs::File::create(&path).expect("could not create");
        let mut fw = FileWatcher::new(&path).expect("must be able to create");

        let mut fwfiles: Vec<FWFile> = vec![];
        fwfiles.push(FWFile::new());
        let mut read_index = 0;
        for action in actions.iter() {
            match *action {
                FWAction::DeleteFile => {
                    let _ = fs::remove_file(&path);
                    assert!(!path.exists());
                    fwfiles[0].reset();
                    break;
                }
                FWAction::TruncateFile => {}
                FWAction::Pause(ps) => time::delay(ps),
                FWAction::Exit => break,
                FWAction::WriteLine(ref s) => {
                    fwfiles[0].write_line(s);
                    assert!(fp.write(s.as_bytes()).is_ok());
                    assert!(fp.write("\n".as_bytes()).is_ok());
                    assert!(fp.flush().is_ok());
                }
                FWAction::RotateFile => {
                    let mut new_path = path.clone();
                    new_path.set_extension("log.1");
                    match fs::rename(&path, &new_path) {
                        Ok(_) => {}
                        Err(_) => {
                            assert!(false);
                        }
                    }
                    fp = fs::File::create(&path).expect("could not create");
                    fwfiles.insert(0, FWFile::new());
                    read_index += 1;
                }
                FWAction::Read => {
                    let mut buf = String::new();
                    let mut attempts = 10;
                    while attempts > 0 {
                        match fw.read_line(&mut buf) {
                            Err(_) => {
                                unreachable!();
                            }
                            Ok(0) => {
                                attempts -= 1;
                                assert!(fwfiles[read_index].read_line().is_none());
                                read_index = 0;
                                continue;
                            }
                            Ok(sz) => {
                                let exp;
                                loop {
                                    let psv = fwfiles[read_index].read_line();
                                    if psv.is_none() {
                                        assert!(read_index != 0);
                                        read_index = 0;
                                    } else {
                                        exp = psv.unwrap();
                                        break;
                                    }
                                }
                                assert_eq!(exp, buf);
                                assert_eq!(sz, buf.len());
                                buf.clear();
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
        fn inner(actions: Vec<FWAction>) -> TestResult {
            experiment_no_truncations(actions);
            TestResult::passed()
        }
        QuickCheck::new()
            .tests(10000)
            .max_tests(100000)
            .quickcheck(inner as fn(Vec<FWAction>) -> TestResult);
    }

    #[test]
    fn file_watcher_with_truncation() {
        fn inner(actions: Vec<FWAction>) -> TestResult {
            experiment(actions);
            TestResult::passed()
        }
        QuickCheck::new()
            .tests(10000)
            .max_tests(100000)
            .quickcheck(inner as fn(Vec<FWAction>) -> TestResult);
    }
}

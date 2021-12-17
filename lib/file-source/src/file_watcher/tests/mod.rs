mod experiment;
mod experiment_no_truncations;

use std::str;

use quickcheck::{Arbitrary, Gen};

// Welcome.
//
// This suite of tests is structured as an interpreter of file system
// actions. You'll find two interpreters here, `experiment` and
// `experiment_no_truncations`. These differ in one key respect: the later
// does not interpret the 'truncation' instruction.
//
// What do I mean by all this? Well, what we're trying to do is validate the
// behaviour of the file_watcher in the presence of arbitrary file-system
// actions. These actions we call `FWAction`.
#[derive(Clone, Debug)]
pub enum FileWatcherAction {
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
// varies slightly in the presence of truncation vs. not -- and FWFile.
pub struct FileWatcherFile {
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
impl FileWatcherFile {
    pub fn new() -> FileWatcherFile {
        FileWatcherFile {
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

    /// Read a line from storage, if a line is available to be read.
    pub fn read_line(&mut self) -> Option<String> {
        // FWFile mimics a unix file being read in a buffered fashion,
        // driven by file_watcher. We _have_ to keep on top of where the
        // reader's read index -- called read_idx -- is between reads and
        // the size of the file -- called previous_read_size -- in the event
        // of truncation.
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
        // truncated and then filled back in before a read could occur
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
        // Seek end_idx forward until we hit the newline character.
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
            Some(ret.to_string())
        } else {
            None
        }
    }
}

impl Arbitrary for FileWatcherAction {
    fn arbitrary(g: &mut Gen) -> FileWatcherAction {
        let i: usize = *g.choose(&(0..100).collect::<Vec<_>>()).unwrap();
        match i {
            // These weights are more or less arbitrary. 'Pause' maybe
            // doesn't have a use but we keep it in place to allow for
            // variations in file-system flushes.
            0..=50 => {
                const GEN_ASCII_STR_CHARSET: &[u8] =
                    b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
                let ln_sz = *g.choose(&(1..32).collect::<Vec<_>>()).unwrap();
                FileWatcherAction::WriteLine(
                    std::iter::repeat_with(|| *g.choose(GEN_ASCII_STR_CHARSET).unwrap())
                        .take(ln_sz)
                        .map(|v| -> char { v.into() })
                        .collect(),
                )
            }
            51..=69 => FileWatcherAction::Read,
            70..=75 => {
                let pause = *g.choose(&(1..3).collect::<Vec<_>>()).unwrap();
                FileWatcherAction::Pause(pause)
            }
            76..=85 => FileWatcherAction::RotateFile,
            86..=90 => FileWatcherAction::TruncateFile,
            91..=95 => FileWatcherAction::DeleteFile,
            _ => FileWatcherAction::Exit,
        }
    }
}

#[inline]
pub fn delay(attempts: u32) {
    let delay = match attempts {
        0 => return,
        1 => 1,
        2 => 4,
        3 => 8,
        4 => 16,
        5 => 32,
        6 => 64,
        7 => 128,
        8 => 256,
        _ => 512,
    };
    let sleep_time = std::time::Duration::from_millis(delay as u64);
    std::thread::sleep(sleep_time);
}

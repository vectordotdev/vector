#[macro_use]
extern crate dlopen_derive;

use dlopen::wrapper::{Container, WrapperApi};
use libc::size_t;
use std::collections::HashMap;
use std::io::{Error as IOError, Result as IOResult};
use std::iter;
use std::os::raw::c_int;
use std::ptr::null_mut;

const SD_JOURNAL_LOCAL_ONLY: c_int = 1;
const SD_JOURNAL_RUNTIME_ONLY: c_int = 2;

#[allow(non_camel_case_types)]
enum sd_journal {}

#[derive(WrapperApi)]
struct LibSystemd {
    sd_journal_open: extern "C" fn(ret: *mut *mut sd_journal, flags: c_int) -> c_int,
    sd_journal_close: extern "C" fn(j: *mut sd_journal),
    sd_journal_next: extern "C" fn(j: *mut sd_journal) -> c_int,
    sd_journal_seek_head: extern "C" fn(j: *mut sd_journal) -> c_int,
    sd_journal_restart_data: extern "C" fn(j: *mut sd_journal),
    sd_journal_enumerate_data:
        extern "C" fn(j: *mut sd_journal, data: *const *mut u8, l: *mut size_t) -> c_int,
}

fn load_lib() -> Result<Container<LibSystemd>, dlopen::Error> {
    unsafe { Container::load("libsystemd.so") }
}

/// A minimal systemd journald reader.
///
/// Supports only the features that Vector requires: open and read next
/// (iterator).
///
/// This was implemented to support loading the `libsystemd.so` library
/// at run time to prevent adding it as a hard dependency for the
/// static-linked target.
pub struct Journal {
    lib: Container<LibSystemd>,
    journal: *mut sd_journal,
}

unsafe impl Send for Journal {}

pub type Record = HashMap<String, String>;

fn bool_flag<T: Default>(flag: bool, tvalue: T) -> T {
    if flag {
        tvalue
    } else {
        T::default()
    }
}

impl Journal {
    /// Open the journald source for reading.
    ///
    /// Params:
    ///
    /// * local_only: If `true`, include only journal entries
    ///   originating from the current host. Otherwise, include entries
    ///   from all hosts.
    /// * runtime_only: If `true`, include only journal entries from
    ///   volatile journal files, excluding those stored on persistent
    ///   storage. Otherwise, include persistent records.
    pub fn open(local_only: bool, runtime_only: bool) -> Result<Journal, Error> {
        // Each Journal structure gets their own handle to the library,
        // but I couldn't figure out how to make lazy_static work.
        let lib = load_lib()?;

        let flags = bool_flag(local_only, SD_JOURNAL_LOCAL_ONLY)
            | bool_flag(runtime_only, SD_JOURNAL_RUNTIME_ONLY);
        let mut journal = null_mut();
        sd_result(lib.sd_journal_open(&mut journal, flags))?;
        sd_result(lib.sd_journal_seek_head(journal))?;
        Ok(Journal { lib, journal })
    }

    /// Fetch the current record structure. `libsystemd` reads journal
    /// records in two steps -- first advance to the next record and
    /// then fetch the fields of the current record. This accomplishes
    /// the second part of that process.
    pub fn current_record(&mut self) -> IOResult<Record> {
        self.lib.sd_journal_restart_data(self.journal);

        iter::from_fn(|| {
            let mut size: size_t = 0;
            let data: *mut u8 = null_mut();

            match sd_result(
                self.lib
                    .sd_journal_enumerate_data(self.journal, &data, &mut size),
            ) {
                Err(err) => Some(Err(err)),
                Ok(0) => None,

                Ok(_) => {
                    let b = unsafe { std::slice::from_raw_parts(data, size as usize) };
                    let field = String::from_utf8_lossy(b);
                    let eq = field.find('=').unwrap();
                    Some(Ok((field[..eq].into(), field[eq + 1..].into())))
                }
            }
        })
        .collect::<IOResult<Record>>()
    }
}

impl Iterator for Journal {
    type Item = IOResult<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        match sd_result(self.lib.sd_journal_next(self.journal)) {
            Err(err) => Some(Err(err)),
            Ok(0) => None,
            _ => Some(self.current_record()),
        }
    }
}

fn sd_result(code: c_int) -> IOResult<c_int> {
    match code {
        _ if code < 0 => Err(IOError::from_raw_os_error(-code)),
        _ => Ok(code),
    }
}

/// Error type for functions that return more than just
/// `std::io::Error`.
#[derive(Debug)]
pub enum Error {
    IOError(IOError),
    DLOpenError(dlopen::Error),
}

impl From<dlopen::Error> for Error {
    fn from(err: dlopen::Error) -> Error {
        Error::DLOpenError(err)
    }
}

impl From<IOError> for Error {
    fn from(err: IOError) -> Error {
        Error::IOError(err)
    }
}

impl std::error::Error for Error {}

impl std::fmt::Display for Error {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        match self {
            Error::IOError(err) => write!(fmt, "I/O Error: {}", err),
            Error::DLOpenError(err) => write!(fmt, "dlopen Error: {}", err),
        }
    }
}

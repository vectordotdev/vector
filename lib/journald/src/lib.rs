#[macro_use]
extern crate dlopen_derive;

use dlopen::wrapper::{Container, WrapperApi};
use libc::{free, size_t, strlen};
use std::collections::HashMap;
use std::ffi::CString;
use std::io;
use std::iter;
use std::os::raw::{c_char, c_int, c_uchar, c_void};
use std::ptr::null_mut;

const SD_JOURNAL_LOCAL_ONLY: c_int = 1;
const SD_JOURNAL_RUNTIME_ONLY: c_int = 2;

#[allow(non_camel_case_types)]
enum sd_journal {}

#[allow(non_camel_case_types)]
#[repr(C)]
#[derive(Clone, Copy)]
struct sd_id128_t {
    pub bytes: [u8; 16],
}

#[derive(WrapperApi)]
struct LibSystemd {
    sd_id128_get_boot: extern "C" fn(ret: *mut sd_id128_t) -> c_int,
    sd_id128_to_string: extern "C" fn(sd: sd_id128_t, s: *mut [c_uchar; 33]) -> *mut c_char,

    sd_journal_add_match:
        extern "C" fn(j: *mut sd_journal, data: *const c_void, size: size_t) -> c_int,
    sd_journal_close: extern "C" fn(j: *mut sd_journal),
    sd_journal_enumerate_data:
        extern "C" fn(j: *mut sd_journal, data: *const *mut u8, l: *mut size_t) -> c_int,
    sd_journal_flush_matches: extern "C" fn(j: *mut sd_journal),
    sd_journal_get_cursor: extern "C" fn(j: *mut sd_journal, cursor: *const *mut c_char) -> c_int,
    sd_journal_next: extern "C" fn(j: *mut sd_journal) -> c_int,
    sd_journal_open: extern "C" fn(ret: *mut *mut sd_journal, flags: c_int) -> c_int,
    sd_journal_restart_data: extern "C" fn(j: *mut sd_journal),
    sd_journal_seek_cursor: extern "C" fn(j: *mut sd_journal, cursor: *const c_char) -> c_int,
    sd_journal_seek_head: extern "C" fn(j: *mut sd_journal) -> c_int,
    sd_journal_seek_monotonic_usec:
        extern "C" fn(j: *mut sd_journal, boot_id: sd_id128_t, usec: u64) -> c_int,
    sd_journal_test_cursor: extern "C" fn(j: *mut sd_journal, cursor: *const c_char) -> c_int,
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
    pub fn current_record(&mut self) -> io::Result<Record> {
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
        .collect::<io::Result<Record>>()
    }

    pub fn cursor(&self) -> io::Result<String> {
        let mut cursor: *mut c_char = null_mut();
        sd_result(self.lib.sd_journal_get_cursor(self.journal, &mut cursor))?;
        Ok(into_string(cursor).unwrap())
    }

    pub fn seek_cursor(&self, cursor: &str) -> io::Result<()> {
        let cursor = CString::new(cursor)?;
        sd_result(
            self.lib
                .sd_journal_seek_cursor(self.journal, cursor.as_ptr()),
        )?;
        Ok(())
    }

    /// Limit the returned records to those from the current boot.
    pub fn setup_current_boot(&self) -> io::Result<()> {
        let mut boot_id = sd_id128_t { bytes: [0; 16] };
        sd_result(self.lib.sd_id128_get_boot(&mut boot_id))?;

        let mut boot_str = [0u8; 33];
        self.lib.sd_id128_to_string(boot_id, &mut boot_str);
        let boot_str = String::from_utf8_lossy(&boot_str[0..32]);

        // Seek to the first record of the current boot.
        sd_result(
            self.lib
                .sd_journal_seek_monotonic_usec(self.journal, boot_id, 0),
        )?;

        // Journald does not guarantee that the following records are
        // only from the current boot, so a filter is also needed.
        self.lib.sd_journal_flush_matches(self.journal);
        let filter = format!("_BOOT_ID={}", boot_str);
        sd_result(self.lib.sd_journal_add_match(
            self.journal,
            filter.as_ptr() as *const c_void,
            filter.len(),
        ))?;

        Ok(())
    }
}

impl Iterator for Journal {
    type Item = io::Result<Record>;

    fn next(&mut self) -> Option<Self::Item> {
        match sd_result(self.lib.sd_journal_next(self.journal)) {
            Err(err) => Some(Err(err)),
            Ok(0) => None,
            _ => Some(self.current_record()),
        }
    }
}

fn sd_result(code: c_int) -> io::Result<c_int> {
    match code {
        _ if code < 0 => Err(io::Error::from_raw_os_error(-code)),
        _ => Ok(code),
    }
}

/// Turn a C `char*` into a String, and free the source
fn into_string(ptr: *mut c_char) -> Option<String> {
    if ptr.is_null() {
        None
    } else {
        Some(unsafe {
            let result =
                String::from_utf8_lossy(&std::slice::from_raw_parts(ptr as *mut u8, strlen(ptr)))
                    .into_owned();
            free(ptr as *mut c_void);
            result
        })
    }
}

/// Error type for functions that return more than just
/// `std::io::Error`.
#[derive(Debug)]
pub enum Error {
    IOError(io::Error),
    DLOpenError(dlopen::Error),
}

impl From<dlopen::Error> for Error {
    fn from(err: dlopen::Error) -> Error {
        Error::DLOpenError(err)
    }
}

impl From<io::Error> for Error {
    fn from(err: io::Error) -> Error {
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

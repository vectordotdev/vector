//! FIXME: A workaround to fix <https://github.com/vectordotdev/vector/issues/1480> resulting from <https://github.com/rust-lang/rust/issues/63010>
//! Most of code is cribbed directly from the Rust stdlib and ported to work with winapi.
//!
//! In stdlib imported code, warnings are allowed.

use std::fs::File;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::{mem::zeroed, ptr};

#[cfg(windows)]
use winapi::shared::minwindef::DWORD;
#[cfg(windows)]
use winapi::um::{
    fileapi::GetFileInformationByHandle, fileapi::BY_HANDLE_FILE_INFORMATION,
    ioapiset::DeviceIoControl, winioctl::FSCTL_GET_REPARSE_POINT,
    winnt::FILE_ATTRIBUTE_REPARSE_POINT, winnt::MAXIMUM_REPARSE_DATA_BUFFER_SIZE,
};

#[cfg(not(windows))]
pub trait PortableFileExt {
    fn portable_dev(&self) -> std::io::Result<u64>;
    fn portable_ino(&self) -> std::io::Result<u64>;
}

#[cfg(windows)]
pub trait PortableFileExt: std::os::windows::io::AsRawHandle {
    fn portable_dev(&self) -> std::io::Result<u64>;
    fn portable_ino(&self) -> std::io::Result<u64>;
    // This code is from the Rust stdlib https://github.com/rust-lang/rust/blob/30ddb5a8c1e85916da0acdc665d6a16535a12dd6/src/libstd/sys/windows/fs.rs#L458-L478
    #[allow(unused_assignments, unused_variables)]
    fn reparse_point<'a>(
        &self,
        space: &'a mut [u8; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize],
    ) -> std::io::Result<(DWORD, &'a REPARSE_DATA_BUFFER)> {
        unsafe {
            let mut bytes = 0;
            cvt({
                DeviceIoControl(
                    self.as_raw_handle(),
                    FSCTL_GET_REPARSE_POINT,
                    ptr::null_mut(),
                    0,
                    space.as_mut_ptr() as *mut _,
                    space.len() as DWORD,
                    &mut bytes,
                    ptr::null_mut(),
                )
            })?;
            Ok((bytes, &*(space.as_ptr() as *const REPARSE_DATA_BUFFER)))
        }
    }
    // This code is from the Rust stdlib https://github.com/rust-lang/rust/blob/30ddb5a8c1e85916da0acdc665d6a16535a12dd6/src/libstd/sys/windows/fs.rs#L326-L351
    #[allow(unused_assignments, unused_variables)]
    fn get_file_info(&self) -> std::io::Result<BY_HANDLE_FILE_INFORMATION> {
        unsafe {
            let mut info: BY_HANDLE_FILE_INFORMATION = zeroed();
            cvt(GetFileInformationByHandle(self.as_raw_handle(), &mut info))?;
            let mut reparse_tag = 0;
            if info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                let mut b = [0; MAXIMUM_REPARSE_DATA_BUFFER_SIZE as usize];
                if let Ok((_, buf)) = self.reparse_point(&mut b) {
                    reparse_tag = buf.ReparseTag;
                }
            }
            Ok(info)
        }
    }
}

#[cfg(unix)]
impl PortableFileExt for File {
    fn portable_dev(&self) -> std::io::Result<u64> {
        Ok(self.metadata()?.dev())
    }
    fn portable_ino(&self) -> std::io::Result<u64> {
        Ok(self.metadata()?.ino())
    }
}

#[cfg(windows)]
impl PortableFileExt for File {
    fn portable_dev(&self) -> std::io::Result<u64> {
        let info = self.get_file_info()?;
        Ok(info.dwVolumeSerialNumber.into())
    }
    // This is not exactly inode, but it's close. See https://docs.microsoft.com/en-us/windows/win32/api/fileapi/ns-fileapi-by_handle_file_information
    fn portable_ino(&self) -> std::io::Result<u64> {
        let info = self.get_file_info()?;
        // https://github.com/rust-lang/rust/blob/30ddb5a8c1e85916da0acdc665d6a16535a12dd6/src/libstd/sys/windows/fs.rs#L347
        Ok((info.nFileIndexLow as u64) | ((info.nFileIndexHigh as u64) << 32))
    }
}

// This code is from the Rust stdlib https://github.com/rust-lang/rust/blob/a916ac22b9f7f1f0f7aba0a41a789b3ecd765018/src/libstd/sys/windows/c.rs#L380-L386
#[cfg(windows)]
#[allow(dead_code, non_snake_case, non_camel_case_types)]
pub struct REPARSE_DATA_BUFFER {
    pub ReparseTag: libc::c_uint,
    pub ReparseDataLength: libc::c_ushort,
    pub Reserved: libc::c_ushort,
    pub rest: (),
}

// This code is from the Rust stdlib  https://github.com/rust-lang/rust/blob/30ddb5a8c1e85916da0acdc665d6a16535a12dd6/src/libstd/sys/hermit/mod.rs#L141-L143
#[cfg(windows)]
pub fn cvt(result: i32) -> std::io::Result<usize> {
    if result < 0 {
        Err(std::io::Error::from_raw_os_error(-result))
    } else {
        Ok(result as usize)
    }
}

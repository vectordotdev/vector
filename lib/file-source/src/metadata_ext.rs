use std::fs::Metadata;
#[cfg(unix)]
use std::os::unix::fs::MetadataExt;
#[cfg(windows)]
use std::os::windows::fs::MetadataExt;

pub trait PortableMetadataExt {
    fn portable_dev(&self) -> u64;
    fn portable_ino(&self) -> u64;
}

#[cfg(unix)]
impl PortableMetadataExt for Metadata {
    fn portable_dev(&self) -> u64 {
        self.dev()
    }
    fn portable_ino(&self) -> u64 {
        self.ino()
    }
}

#[cfg(windows)]
impl PortableMetadataExt for Metadata {
    fn portable_dev(&self) -> u64 {
        self.volume_serial_number().unwrap_or(0u32) as u64
    }
    fn portable_ino(&self) -> u64 {
        self.file_index().unwrap_or(0u64)
    }
}

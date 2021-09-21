use crate::WhenFull;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub enum Variant {
    Memory {
        max_events: usize,
        when_full: WhenFull,
    },
    Disk {
        max_size: usize,
        when_full: WhenFull,
        data_dir: PathBuf,
        id: String,
    },
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct Id {
    inner: String,
}

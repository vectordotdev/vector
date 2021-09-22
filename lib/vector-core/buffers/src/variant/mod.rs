use crate::WhenFull;

#[cfg(not(feature = "disk-buffer"))]
#[derive(Debug, Clone, Copy)]
pub enum Variant {
    Memory {
        max_events: usize,
        when_full: WhenFull,
    },
}

#[cfg(feature = "disk-buffer")]
#[derive(Debug, Clone)]
pub enum Variant {
    Memory {
        max_events: usize,
        when_full: WhenFull,
    },
    Disk {
        max_size: usize,
        when_full: WhenFull,
        data_dir: std::path::PathBuf,
        id: String,
    },
}

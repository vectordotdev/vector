// We don't want data files to be bigger than 128MB, but we might end up overshooting slightly.
pub const DATA_FILE_TARGET_MAX_SIZE: u64 = 128 * 1024 * 1024;
// There's no particular reason that _has_ to be 8MB, it's just a simple default we've chosen here.
pub const DATA_FILE_MAX_RECORD_SIZE: usize = 8 * 1024 * 1024;

mod global_options;
mod id;
mod log_schema;
pub mod proxy;

pub use global_options::GlobalOptions;
pub use id::ComponentKey;
pub use log_schema::{init_log_schema, log_schema, LogSchema};

pub const MEMORY_BUFFER_DEFAULT_MAX_EVENTS: usize =
    buffers::config::memory_buffer_default_max_events();

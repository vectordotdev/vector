use crate::Role;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf};

/// The base configuration required for a WasmModule.
///
/// If you're designing a module around the WasmModule type, you need to build it with one of these.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct WasmModuleConfig {
    /// The role which the module will play.
    pub role: Role,
    /// The path to the module's `wasm` file.
    pub path: PathBuf,
    /// The cache location where an optimized `so` file shall be placed.
    ///
    /// This folder also stores a `.fingerprints` file that is formatted as a JSON map, matching file paths
    /// to fingerprints.
    pub artifact_cache: PathBuf,
    /// The maximum size of the heap the module may grow to.
    // TODO: The module may also declare it's minimum heap size, and they will be compared before
    //       the module begins processing.
    pub max_heap_memory_size: usize,
    pub options: HashMap<String, serde_json::Value>,
}

impl WasmModuleConfig {
    /// Build a new configuration with the required options set.
    pub fn new(
        role: Role,
        path: impl Into<PathBuf>,
        artifact_cache: impl Into<PathBuf>,
        options: HashMap<String, serde_json::Value>,
        max_heap_memory_size: usize,
    ) -> Self {
        Self {
            role,
            path: path.into(),
            artifact_cache: artifact_cache.into(),
            // The rest should be configured via setters below...
            max_heap_memory_size,
            options,
        }
    }

    /// Set the maximum heap size of the transform to the given value. See `defaults::HEAP_MEMORY_SIZE`.
    pub fn set_max_heap_memory_size(&mut self, max_heap_memory_size: usize) -> &mut Self {
        self.max_heap_memory_size = max_heap_memory_size;
        self
    }
}

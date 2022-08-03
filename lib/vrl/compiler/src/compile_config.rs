use anymap::AnyMap;
use lookup::LookupBuf;
use std::collections::BTreeSet;

pub struct CompileConfig {
    /// Custom context injected by the external environment
    custom: AnyMap,

    read_only_paths: BTreeSet<ReadOnlyPath>,
}

impl CompileConfig {
    /// Get external context data from the external environment.
    #[must_use]
    pub fn get_custom<T: 'static>(&self) -> Option<&T> {
        self.custom.get::<T>()
    }

    /// Get external context data from the external environment.
    pub fn get_custom_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.custom.get_mut::<T>()
    }

    /// Sets the external context data for VRL functions to use.
    pub fn set_custom<T: 'static>(&mut self, data: T) {
        self.custom.insert::<T>(data);
    }

    pub fn custom_mut(&mut self) -> &mut AnyMap {
        &mut self.custom
    }

    #[must_use]
    pub fn is_read_only_event_path(&self, path: &LookupBuf) -> bool {
        self.is_read_only_path(path, PathRoot::Event)
    }

    #[must_use]
    pub fn is_read_only_metadata_path(&self, path: &LookupBuf) -> bool {
        self.is_read_only_path(path, PathRoot::Metadata)
    }

    /// Marks everything as read only. Any mutations on read-only values will result in a
    /// compile time error.
    pub fn set_read_only(&mut self) {
        self.set_read_only_event_path(LookupBuf::root(), true);
        self.set_read_only_metadata_path(LookupBuf::root(), true);
    }

    fn is_read_only_path(&self, path: &LookupBuf, root: PathRoot) -> bool {
        for read_only_path in &self.read_only_paths {
            if read_only_path.root != root {
                continue;
            }

            // any paths that are a parent of read-only paths also can't be modified
            if read_only_path.path.can_start_with(path) {
                return true;
            }

            if read_only_path.recursive {
                if path.can_start_with(&read_only_path.path) {
                    return true;
                }
            } else if path == &read_only_path.path {
                return true;
            }
        }
        false
    }

    /// Adds a path that is considered read only. Assignments to any paths that match
    /// will fail at compile time.
    fn set_read_only_path(&mut self, path: LookupBuf, recursive: bool, root: PathRoot) {
        self.read_only_paths.insert(ReadOnlyPath {
            path,
            recursive,
            root,
        });
    }

    pub fn set_read_only_event_path(&mut self, path: LookupBuf, recursive: bool) {
        self.set_read_only_path(path, recursive, PathRoot::Event);
    }

    pub fn set_read_only_metadata_path(&mut self, path: LookupBuf, recursive: bool) {
        self.set_read_only_path(path, recursive, PathRoot::Metadata);
    }
}

impl Default for CompileConfig {
    fn default() -> Self {
        Self {
            custom: AnyMap::new(),
            read_only_paths: BTreeSet::new(),
        }
    }
}

// temporary until paths can point to metadata
#[derive(Debug, Clone, Ord, Eq, PartialEq, PartialOrd)]
enum PathRoot {
    Event,
    Metadata,
}

#[derive(Debug, Clone, Ord, Eq, PartialEq, PartialOrd)]
struct ReadOnlyPath {
    path: LookupBuf,
    recursive: bool,
    root: PathRoot,
}

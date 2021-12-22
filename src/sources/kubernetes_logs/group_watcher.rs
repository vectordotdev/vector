use std::collections::HashMap;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct FileCounter {
    start_time: Instant,
    lines_read: usize,
}

impl Default for FileCounter {
    fn default() -> Self {
        Self {
            start_time: Instant::now(),
            lines_read: 0,
        }
    }
}

impl FileCounter {
    pub fn reset(&mut self) {
        self.start_time = Instant::now();
        self.lines_read = 0;
    }

    pub fn incr(&mut self) {
        self.lines_read += 1;
    }
}

#[derive(Clone, Debug)]
pub struct GroupWatcher {
    limit: usize,
    current_paths: HashMap<String, FileCounter>,
}

/// Hashmap of Namespace, podname and limit
pub type GroupRuleHash = HashMap<String, HashMap<String, GroupWatcher>>;

impl GroupWatcher {
    pub fn new(rate_limit: usize) -> Self {
        Self {
            limit: rate_limit,
            current_paths: HashMap::new(),
        }
    }

    pub fn get(&self, filename: &str) -> Option<&FileCounter> {
        self.current_paths.get(filename)
    }

    pub fn add(&mut self, filename: &str) {
        self.current_paths
            .insert(filename.to_string(), FileCounter::default());
    }

    pub fn line_limit_reached(&self, filename: &str) -> bool {
        self.get(filename).unwrap().lines_read >= self.limit
    }

    pub fn time_elapsed(&self, filename: &str) -> Duration {
        self.get(filename).unwrap().start_time.elapsed()
    }

    fn get_mut(&mut self, filename: &str) -> Option<&mut FileCounter> {
        self.current_paths.get_mut(filename)
    }

    pub fn reset(&mut self, filename: &str) {
        self.get_mut(filename).unwrap().reset();
    }

    pub fn incr_event(&mut self, filename: &str) {
        self.get_mut(filename).unwrap().incr();
    }
}

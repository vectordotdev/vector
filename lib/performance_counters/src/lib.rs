mod bindings {
    #![allow(non_upper_case_globals)]
    #![allow(non_camel_case_types)]
    #![allow(non_snake_case)]
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub struct Library;

impl Library {
    pub fn new() -> Result<Library, String> {
        (unsafe { bindings::init_counting() } == 0)
            .then(|| Library)
            .ok_or_else(|| "Failed initializing library.".into())
    }

    pub fn print_db_info(&self) {
        unsafe { bindings::print_db_info() };
    }

    pub fn start_counting(&self) -> Result<Counting, String> {
        (unsafe { bindings::start_counting() } == 0)
            .then(|| Counting)
            .ok_or_else(|| "Failed to start counting.".into())
    }
}

pub struct Counting;

impl Counting {
    pub fn get_counters(&self) -> PerformanceCounters {
        PerformanceCounters::new()
    }

    pub fn print_counters(start: &PerformanceCounters, end: &PerformanceCounters) {
        unsafe {
            bindings::print_counters(&start.0 as *const _ as *mut _, &end.0 as *const _ as *mut _)
        }
    }

    pub fn stop(self) {}
}

impl Drop for Counting {
    fn drop(&mut self) {
        unsafe { bindings::stop_counting() };
    }
}

pub struct PerformanceCounters(bindings::counters);

impl PerformanceCounters {
    fn new() -> Self {
        let mut counters = unsafe { bindings::init_counters() };
        unsafe { bindings::get_counters(&mut counters) };
        Self(counters)
    }
}

impl std::ops::Sub for PerformanceCounters {
    type Output = PerformanceCountersResult;

    fn sub(mut self, mut rhs: Self) -> Self::Output {
        Self::Output::from(unsafe { bindings::get_named_counters(&mut rhs.0, &mut self.0) })
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformanceCountersResult {
    pub cycles: u64,
    pub load_store_instructions: u64,
    pub l1_data_load_cache_misses: u64,
    pub l1_data_store_cache_misses: u64,
}

impl From<bindings::named_counters> for PerformanceCountersResult {
    fn from(named_counters: bindings::named_counters) -> Self {
        Self {
            cycles: named_counters.cycles,
            load_store_instructions: named_counters.load_store_instructions,
            l1_data_load_cache_misses: named_counters.l1_data_load_cache_misses,
            l1_data_store_cache_misses: named_counters.l1_data_store_cache_misses,
        }
    }
}

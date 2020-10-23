use super::InternalEvent;
use metrics::gauge;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

pub type OpenTokenDyn = OpenToken<Box<dyn Fn(usize) + 'static + Send>>;

#[derive(Debug)]
pub struct ConnectionOpen {
    pub count: usize,
}

impl InternalEvent for ConnectionOpen {
    fn emit_metrics(&self) {
        gauge!("open_connections", self.count as f64);
    }
}

#[derive(Clone)]
pub struct OpenGauge {
    gauge: Arc<AtomicUsize>,
}

impl OpenGauge {
    pub fn new() -> Self {
        OpenGauge {
            gauge: Arc::default(),
        }
    }

    /// Increments and emits value once created.
    /// Decrements and emits value once dropped.
    pub fn open<E: Fn(usize)>(self, emitter: E) -> OpenToken<E> {
        gauge_add(&self.gauge, 1, &emitter);
        OpenToken {
            gauge: self.gauge,
            emitter,
        }
    }
}

impl Default for OpenGauge {
    fn default() -> Self {
        Self::new()
    }
}

pub struct OpenToken<E: Fn(usize)> {
    gauge: Arc<AtomicUsize>,
    emitter: E,
}

impl<E: Fn(usize)> Drop for OpenToken<E> {
    fn drop(&mut self) {
        gauge_add(&self.gauge, -1, &self.emitter);
    }
}

/// If reporting gauges from multiple threads, they can end up in a wrong order
/// resulting in having wrong value for a prolonged period of time.
/// This function performs a synchronization procedure that corrects that.
fn gauge_add(gauge: &AtomicUsize, add: isize, emitter: impl Fn(usize)) {
    // Lock-free procedure with eventual consistency

    // Initialize value and acquire older writes to gauge metric.
    let mut value = gauge.load(Ordering::Acquire);
    loop {
        let new_value = (value as isize + add) as usize;
        emitter(new_value);
        // Try to update gauge to new value and releasing writes to gauge metric in the process.
        // Otherwise acquire new writes to gauge metric.
        let latest = gauge.compare_and_swap(value, new_value, Ordering::AcqRel);
        if value == latest {
            // Success
            break;
        }
        // Try again with new value
        value = latest;
    }

    // In the worst case scenario we will emit `n^2 / 2` times when there are `n` parallel
    // updates in proggress. This scenario has higher chance of happening during shutdown.
    // In most cases `n` will be small, and futher limited by the number of active threads.
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::drop;
    use std::thread;

    /// If this failes at any run, then the algorithm in `gauge_add` is faulty.
    #[test]
    fn eventually_consistent() {
        let n = 8;
        let m = 1000;
        let gauge = OpenGauge::new();
        let value = Arc::new(AtomicUsize::new(0));

        let handles = (0..n)
            .map(|_| {
                let gauge = gauge.clone();
                let value = Arc::clone(&value);
                thread::spawn(move || {
                    let mut work = 0;
                    for _ in 0..m {
                        let token = gauge
                            .clone()
                            .open(|count| value.store(count, Ordering::Release));
                        // Do some work
                        for i in 0..100 {
                            work += i;
                        }
                        drop(token);
                    }
                    work
                })
            })
            .collect::<Vec<_>>();

        for handle in handles {
            handle.join().unwrap();
        }

        assert_eq!(0, value.load(Ordering::Acquire));
    }
}

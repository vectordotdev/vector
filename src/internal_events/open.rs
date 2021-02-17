use super::InternalEvent;
use metrics::gauge;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

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
    //
    // This function will emit only once the current thread has managed to
    // modify `gauge`. Because of the Acquire/Release semantics of success used
    // here it is guaranteed that emission will sequence after the `gauge`
    // modification but there is not guarantee of the ordering of the call to
    // `emitter` relative to other threads.
    //
    // The previous implementation of this function guaranteed a worst case
    // emission of `n^2 / 2` times, where `n` is the number of parallel updates
    // in progress. This was achieved by bounding the call to `emitter` in an
    // Acquire/AcqRel pair, equivalent to Acquire / Release.
    //
    // Current implementation will load `value` with Relaxed semantics except in
    // the case where `compare_exchange_weak` succeeds in which the load of
    // `value` is converted to an Acquire fence.
    let mut value = gauge.load(Ordering::Relaxed);
    loop {
        let new_value = (value as isize + add) as usize;
        // Try to update gauge to new value and releasing writes to gauge metric
        // in the process.  Otherwise acquire new writes to gauge metric.
        //
        // When `compare_exchange_weak` returns Ok our `new_value` is now the
        // current value in memory across all CPUs. When the return is Err we
        // retry with the now current value.
        match gauge.compare_exchange_weak(value, new_value, Ordering::AcqRel, Ordering::Relaxed) {
            Ok(_) => {
                emitter(new_value);
                break;
            }
            Err(x) => value = x,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::mem::drop;
    use std::thread;

    /// If this fails at any run, then the algorithm in `gauge_add` is faulty.
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

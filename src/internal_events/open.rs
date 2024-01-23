use std::{
    hint,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

use metrics::gauge;
use vector_lib::internal_event::InternalEvent;

#[derive(Debug)]
pub struct ConnectionOpen {
    pub count: usize,
}

impl InternalEvent for ConnectionOpen {
    fn emit(self) {
        gauge!("open_connections", self.count as f64);
    }
}

#[derive(Debug)]
pub struct EndpointsActive {
    pub count: usize,
}

impl InternalEvent for EndpointsActive {
    fn emit(self) {
        gauge!("active_endpoints", self.count as f64);
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

    #[cfg(all(feature = "sources-utils-net-unix", unix))]
    pub fn any_open(&self) -> bool {
        self.gauge.load(Ordering::Acquire) != 0
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
    // The goal of this function is to properly sequence calls to `emitter` from
    // multiple threads. It is possible that `emitter` will be called multiple
    // times -- worst case, `n^2 / 2` times where `n` is the number of parallel
    // peers -- but this is acceptable.
    //
    // The implementation here is a spin lock on the `gauge` value with the
    // critical section being solely for updating the `gauge` value by `add` and
    // calling `emitter`. If we suffer priority inversion at higher peer counts
    // we might consider the use of a mutex, which will participate in the OS's
    // scheduler. See [this
    // post](https://matklad.github.io/2020/01/02/spinlocks-considered-harmful.html)
    // for details if you're working on something like that and need background.
    //
    // The order of calls to `emitter` are not guaranteed but it is guaranteed
    // that the most recent holder of the lock will be the most recent caller of
    // `emitter`.
    let mut value = gauge.load(Ordering::Acquire);
    loop {
        let new_value = (value as isize + add) as usize;
        emitter(new_value);
        // Try to update gauge to new value and releasing writes to gauge metric
        // in the process.  Otherwise acquire new writes to gauge metric.
        //
        // When `compare_exchange_weak` returns Ok our `new_value` is now the
        // current value in memory across all CPUs. When the return is Err we
        // retry with the now current value.
        match gauge.compare_exchange_weak(value, new_value, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => break,
            Err(x) => {
                hint::spin_loop();
                value = x;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{mem::drop, thread};

    use super::*;

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

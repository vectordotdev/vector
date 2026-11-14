use std::{
    hint::black_box,
    sync::atomic::{AtomicU64, Ordering},
    thread,
    time::Instant,
};

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn main() {
    assert_eq!(std::mem::size_of::<usize>(), 4);
    let start = Instant::now();
    thread::spawn(|| {
        assert_eq!(COUNTER.fetch_add(1, Ordering::SeqCst), 0);
        assert_eq!(
            COUNTER.compare_exchange(1, 2, Ordering::SeqCst, Ordering::SeqCst),
            Ok(1)
        );
    })
    .join()
    .expect("run ARM32 atomic operations on a native thread");
    assert_eq!(COUNTER.load(Ordering::SeqCst), 2);
    assert!(Instant::now() >= start);
    assert_eq!(black_box(3.25_f64) * black_box(2.0_f64), 6.5);
}

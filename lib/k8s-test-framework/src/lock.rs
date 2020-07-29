use once_cell::sync::OnceCell;
use std::sync::{Mutex, MutexGuard};

/// A shared lock to use commonly among the tests.
/// The goal is to guranatee that only one test is executing concurrently, since
/// tests use a shared resource - a k8s cluster - and will conflict with each
/// other unless they're executing sequentially.
pub fn lock() -> MutexGuard<'static, ()> {
    static INSTANCE: OnceCell<Mutex<()>> = OnceCell::new();
    match INSTANCE.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        // Ignore poison error.
        Err(err) => err.into_inner(),
    }
}

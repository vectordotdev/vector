use std::sync::{Mutex, MutexGuard};

/// A shared lock to use commonly among the tests.
/// The goal is to guarantee that only one test is executing concurrently, since
/// tests use a shared resource - a k8s cluster - and will conflict with each
/// other unless they're executing sequentially.
pub fn lock() -> MutexGuard<'static, ()> {
    static INSTANCE: Mutex<()> = Mutex::new(());
    match INSTANCE.lock() {
        Ok(guard) => guard,
        // Ignore poison error.
        Err(err) => err.into_inner(),
    }
}

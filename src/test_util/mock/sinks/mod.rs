use crate::config::SinkDescription;

mod basic;
pub use self::basic::BasicSinkConfig;

mod error;
pub use self::error::ErrorSinkConfig;

mod panic;
pub use self::panic::PanicSinkConfig;

inventory::submit! {
    SinkDescription::new::<BasicSinkConfig>("test_basic")
}

inventory::submit! {
    SinkDescription::new::<ErrorSinkConfig>("test_error")
}

inventory::submit! {
    SinkDescription::new::<PanicSinkConfig>("test_panic")
}

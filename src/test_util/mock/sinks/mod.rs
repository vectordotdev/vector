mod backpressure;
pub use self::backpressure::BackpressureSinkConfig;

mod basic;
pub use self::basic::BasicSinkConfig;

mod error;
pub use self::error::ErrorSinkConfig;

mod oneshot;
pub use self::oneshot::OneshotSinkConfig;

mod panic;
pub use self::panic::PanicSinkConfig;

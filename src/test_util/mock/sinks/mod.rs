mod basic;
pub use self::basic::BasicSinkConfig;

mod error;
pub use self::error::ErrorSinkConfig;

mod panic;
pub use self::panic::PanicSinkConfig;

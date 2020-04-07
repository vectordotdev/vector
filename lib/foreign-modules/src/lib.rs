#[cfg(feature = "guest")]
pub mod guest;
#[cfg(feature = "host")]
pub mod host;

pub trait Role {}

pub mod roles {
    use super::Role;

    pub struct Sink;
    impl Role for Sink {}
    pub struct Source;
    impl Role for Source {}
    pub struct Transform;
    impl Role for Transform {}
}

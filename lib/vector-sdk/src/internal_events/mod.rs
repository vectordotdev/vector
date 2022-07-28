pub mod common;
pub mod decoder;
pub mod prelude;

#[cfg(test)]
#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        vector_common::internal_event::emit(vector_common::internal_event::DefaultName {
            event: $event,
            name: stringify!($event),
        })
    };
}

#[cfg(not(test))]
#[macro_export]
macro_rules! emit {
    ($event:expr) => {
        vector_common::internal_event::emit($event)
    };
}

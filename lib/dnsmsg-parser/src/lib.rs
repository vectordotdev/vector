#![deny(warnings)]
#![deny(clippy::unwrap_used)]
#![warn(
    missing_debug_implementations,
    rust_2018_idioms,
    unreachable_pub,
    non_snake_case,
    non_upper_case_globals
)]

pub mod dns_message;
pub mod dns_message_parser;
pub mod ede;

#![allow(clippy::clone_on_ref_ptr)]

pub use proto::collector::logs::v1 as LogService;
pub use proto::common::v1 as Common;
pub use proto::logs::v1 as Logs;
pub use proto::resource::v1::Resource;

pub mod convert;
pub mod proto;

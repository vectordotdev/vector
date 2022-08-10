mod client;
mod metas;

mod proto {
    include!(concat!(env!("OUT_DIR"), "/datadog.config.rs"));
}

type Version = u64;

pub use client::{Client, Config};

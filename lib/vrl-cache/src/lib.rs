#![deny(warnings)]

pub mod cache_delete;
pub mod cache_get;
pub mod cache_put;
pub mod caches;

mod internal_events;
mod vrl_util;

pub use caches::VrlCacheRegistry;
use vrl::compiler::Function;

pub fn vrl_functions() -> Vec<Box<dyn Function>> {
    vec![
        Box::new(cache_get::CacheGet) as _,
        Box::new(cache_put::CachePut) as _,
        Box::new(cache_delete::CacheDelete) as _,
    ]
}

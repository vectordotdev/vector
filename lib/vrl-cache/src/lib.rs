#![deny(warnings)]

pub mod cache_get;
pub mod cache_set;
pub mod caches;

mod vrl_util;

pub use caches::VrlCacheRegistry;
use vrl::compiler::Function;

pub fn vrl_functions() -> Vec<Box<dyn Function>> {
    vec![
        Box::new(cache_get::CacheGet) as _,
        Box::new(cache_set::CacheSet) as _,
    ]
}

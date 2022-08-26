use crate::config::TransformDescription;

mod basic;
pub use self::basic::BasicTransformConfig;

inventory::submit! {
    TransformDescription::new::<BasicTransformConfig>("test_basic")
}

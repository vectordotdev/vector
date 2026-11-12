/// A component that can generate a default configuration for itself.
pub trait GenerateConfig {
    fn generate_config() -> serde_json::Value;
}

#[macro_export]
macro_rules! impl_generate_config_from_default {
    ($type:ty) => {
        impl $crate::component::GenerateConfig for $type {
            fn generate_config() -> serde_json::Value {
                serde_json::to_value(&Self::default()).unwrap()
            }
        }
    };
}

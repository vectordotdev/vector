/// A component that can generate a default configuration for itself.
pub trait GenerateConfig {
    fn generate_config() -> toml::Value;
}

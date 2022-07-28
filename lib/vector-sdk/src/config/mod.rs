pub mod sink;

pub trait GenerateConfig {
    fn generate_config() -> toml::Value;
}

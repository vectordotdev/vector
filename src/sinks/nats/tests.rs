use super::config::NatsSinkConfig;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<NatsSinkConfig>();
}

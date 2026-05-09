use super::config::IggySinkConfig;

#[test]
fn generate_config() {
    crate::test_util::test_generate_config::<IggySinkConfig>();
}

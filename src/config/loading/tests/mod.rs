mod bool_or_struct;
mod compatibility;
mod metric_tags;
#[cfg(feature = "sources-demo_logs")]
mod parse_first;

// Replace the fixture marker with a name verified absent from the environment.
// Avoid mutating process-wide variables while tests may be running concurrently.
fn with_absent_env_var(input: &str) -> String {
    let name = loop {
        let name = format!("VECTOR_TEST_COMPAT_{}", crate::test_util::random_string(16));
        if std::env::var_os(&name).is_none() {
            break name;
        }
    };
    input.replace("VECTOR_TEST_COMPAT_UNSET", &name)
}

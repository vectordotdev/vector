use std::{future::Future, path::Path, str::FromStr};

use once_cell::sync::Lazy;
use temp_dir::TempDir;
use tracing_fluent_assertions::{AssertionRegistry, AssertionsLayer};
use tracing_subscriber::{filter::LevelFilter, layer::SubscriberExt, Layer, Registry};
use vector_common::finalization::{EventStatus, Finalizable};

#[macro_export]
macro_rules! assert_file_does_not_exist_async {
    ($file_path:expr) => {{
        let result = tokio::fs::metadata($file_path).await;
        assert!(result.is_err());
        assert_eq!(
            std::io::ErrorKind::NotFound,
            result.expect_err("is_err() was true").kind(),
            "got unexpected error kind"
        );
    }};
}

#[macro_export]
macro_rules! assert_file_exists_async {
    ($file_path:expr) => {{
        let result = tokio::fs::metadata($file_path).await;
        assert!(result.is_ok());
        assert!(
            result.expect("is_ok() was true").is_file(),
            "path exists but is not file"
        );
    }};
}

#[macro_export]
macro_rules! await_timeout {
    ($fut:expr, $secs:expr) => {{
        tokio::time::timeout(std::time::Duration::from_secs($secs), $fut)
            .await
            .expect("future should not timeout")
    }};
}

/// Run a future with a temporary directory.
///
/// # Panics
///
/// Will panic if function cannot create a temp directory.
pub async fn with_temp_dir<F, Fut, V>(f: F) -> V
where
    F: FnOnce(&Path) -> Fut,
    Fut: Future<Output = V>,
{
    let buf_dir = TempDir::with_prefix("vector-buffers")
        .expect("cannot recover from failure to create temp dir");
    f(buf_dir.path()).await
}

pub fn install_tracing_helpers() -> AssertionRegistry {
    // TODO: This installs the assertions layer globally, so all tests will run through it.  Since
    // most of the code being tested overlaps, individual tests should wrap their async code blocks
    // with a unique span that can be matched on specifically with
    // `AssertionBuilder::with_parent_name`.
    //
    // TODO: We also need a better way of wrapping our test functions in their own parent spans, for
    // the purpose of isolating their assertions.  Right now, we do it with a unique string that we
    // have set to the test function name, but this is susceptible to being copypasta'd
    // unintentionally, thus letting assertions bleed into other tests.
    //
    // Maybe we should add a helper method to `tracing-fluent-assertions` for generating a
    // uniquely-named span that can be passed directly to the assertion builder methods, then it's a
    // much tighter loop.
    //
    // TODO: At some point, we might be able to write a simple derive macro that does this for us, and
    // configures the other necessary bits, but for now.... by hand will get the job done.
    static ASSERTION_REGISTRY: Lazy<AssertionRegistry> = Lazy::new(|| {
        let assertion_registry = AssertionRegistry::default();
        let assertions_layer = AssertionsLayer::new(&assertion_registry);

        // Constrain the actual output layer to the normal RUST_LOG-based control mechanism, so that
        // assertions can run unfettered but without also spamming the console with logs.
        let fmt_filter = std::env::var("RUST_LOG")
            .map_err(|_| ())
            .and_then(|s| LevelFilter::from_str(s.as_str()).map_err(|_| ()))
            .unwrap_or(LevelFilter::OFF);
        let fmt_layer = tracing_subscriber::fmt::layer()
            .with_ansi(true)
            .with_span_events(tracing_subscriber::fmt::format::FmtSpan::FULL)
            .with_test_writer()
            .with_filter(fmt_filter);

        let base_subscriber = Registry::default();
        let subscriber = base_subscriber.with(assertions_layer).with(fmt_layer);

        tracing::subscriber::set_global_default(subscriber).unwrap();
        assertion_registry
    });

    ASSERTION_REGISTRY.clone()
}

pub(crate) async fn acknowledge(mut event: impl Finalizable) {
    event
        .take_finalizers()
        .update_status(EventStatus::Delivered);
    // Finalizers are implicitly dropped here, sending the status update.
    tokio::task::yield_now().await;
}

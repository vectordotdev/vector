#[cfg(any(
    feature = "postgres_sink-integration-tests",
    feature = "postgresql_metrics-integration-tests"
))]
pub mod postgres {
    use std::path::PathBuf;

    #[must_use]
    pub fn pg_host() -> String {
        std::env::var("PG_HOST").unwrap_or_else(|_| "localhost".into())
    }

    // https://github.com/vectordotdev/vector/issues/23659
    #[allow(
        clippy::missing_panics_doc,
        reason = "Audit and document the existing panic conditions separately from lint enforcement."
    )]
    pub fn pg_socket() -> PathBuf {
        std::env::var("PG_SOCKET").map_or_else(
            |_| {
                let current_dir = std::env::current_dir().unwrap();
                current_dir
                    .join("tests")
                    .join("data")
                    .join("postgresql-local-socket")
            },
            PathBuf::from,
        )
    }

    #[must_use]
    pub fn pg_url() -> String {
        std::env::var("PG_URL")
            .unwrap_or_else(|_| format!("postgres://vector:vector@{}/postgres", pg_host()))
    }
}

#[cfg(any(
    feature = "postgres_sink-integration-tests",
    feature = "postgresql_metrics-integration-tests"
))]
pub mod postgres {
    use std::path::PathBuf;

    pub fn pg_host() -> String {
        std::env::var("PG_HOST").unwrap_or_else(|_| "localhost".into())
    }

    pub fn pg_socket() -> PathBuf {
        std::env::var("PG_SOCKET")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let current_dir = std::env::current_dir().unwrap();
                current_dir
                    .join("tests")
                    .join("data")
                    .join("postgresql-local-socket")
            })
    }

    pub fn pg_url() -> String {
        std::env::var("PG_URL")
            .unwrap_or_else(|_| format!("postgres://vector:vector@{}/postgres", pg_host()))
    }
}

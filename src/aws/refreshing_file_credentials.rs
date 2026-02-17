//! Refreshing file-based credentials provider for AWS.
//!
//! This module provides a credentials provider that periodically re-reads
//! credentials from a file, solving the issue where `ProfileFileCredentialsProvider`
//! caches credentials indefinitely and doesn't pick up file changes.
//!
//! See: https://github.com/vectordotdev/vector/issues/18591

use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

use aws_config::profile::ProfileFileCredentialsProvider;
use aws_credential_types::{
    provider::{self, future, ProvideCredentials},
    Credentials,
};
use aws_runtime::env_config::file::{EnvConfigFileKind, EnvConfigFiles};
use aws_smithy_runtime_api::client::http::SharedHttpClient;
use aws_types::region::Region;
use tokio::sync::RwLock;

/// Default interval between credential file re-reads (5 minutes).
pub const DEFAULT_REFRESH_INTERVAL: Duration = Duration::from_secs(300);

/// Minimum allowed refresh interval (30 seconds).
pub const MIN_REFRESH_INTERVAL: Duration = Duration::from_secs(30);

/// A credentials provider that periodically re-reads credentials from a file.
///
/// Unlike `ProfileFileCredentialsProvider`, this provider will re-read the
/// credentials file when:
/// 1. The cached credentials are expired (based on `expiration` field)
/// 2. More than `refresh_interval` has passed since the last file read
///
/// This is particularly useful for long-running processes in Kubernetes
/// environments where credentials files are updated by external processes
/// (e.g., IRSA token refresh, credential rotation).
#[derive(Debug)]
pub struct RefreshingFileCredentialsProvider {
    credentials_file: PathBuf,
    profile: String,
    region: Option<Region>,
    http_client: SharedHttpClient,
    refresh_interval: Duration,
    state: Arc<RwLock<ProviderState>>,
}

#[derive(Debug)]
struct ProviderState {
    cached_credentials: Option<Credentials>,
    last_refresh: Option<Instant>,
}

impl ProviderState {
    fn new() -> Self {
        Self {
            cached_credentials: None,
            last_refresh: None,
        }
    }

    fn needs_refresh(&self, refresh_interval: Duration) -> bool {
        match (&self.cached_credentials, self.last_refresh) {
            // No credentials cached - need to load
            (None, _) => true,
            // Check if credentials are expired
            (Some(creds), _) if Self::is_expired(creds) => true,
            // Check if refresh interval has passed
            (_, Some(last)) => last.elapsed() >= refresh_interval,
            // No last refresh time recorded - refresh to be safe
            (_, None) => true,
        }
    }

    fn is_expired(credentials: &Credentials) -> bool {
        credentials
            .expiry()
            .map(|expiry| {
                let now = std::time::SystemTime::now();
                // Consider credentials expired if they expire within the next 5 minutes
                // This provides a buffer for clock skew and network latency
                let buffer = Duration::from_secs(300);
                match expiry.duration_since(now) {
                    Ok(remaining) => remaining < buffer,
                    Err(_) => true, // Already expired
                }
            })
            .unwrap_or(false) // No expiry means long-term credentials
    }
}

impl RefreshingFileCredentialsProvider {
    /// Create a new `RefreshingFileCredentialsProvider`.
    ///
    /// # Arguments
    ///
    /// * `credentials_file` - Path to the AWS credentials file
    /// * `profile` - The profile name to use (e.g., "default")
    /// * `region` - Optional AWS region for STS calls
    /// * `http_client` - HTTP client for making AWS API calls
    /// * `refresh_interval` - How often to re-read the credentials file
    pub fn new(
        credentials_file: PathBuf,
        profile: String,
        region: Option<Region>,
        http_client: SharedHttpClient,
        refresh_interval: Duration,
    ) -> Self {
        // Ensure refresh interval is at least the minimum
        let refresh_interval = refresh_interval.max(MIN_REFRESH_INTERVAL);

        Self {
            credentials_file,
            profile,
            region,
            http_client,
            refresh_interval,
            state: Arc::new(RwLock::new(ProviderState::new())),
        }
    }

    /// Create a builder for configuring the provider.
    pub fn builder() -> RefreshingFileCredentialsProviderBuilder {
        RefreshingFileCredentialsProviderBuilder::default()
    }

    async fn load_credentials(&self) -> provider::Result {
        let profile_files = EnvConfigFiles::builder()
            .with_file(EnvConfigFileKind::Credentials, &self.credentials_file)
            .build();

        let mut provider_config = aws_config::provider_config::ProviderConfig::empty()
            .with_http_client(self.http_client.clone());

        if let Some(region) = &self.region {
            provider_config = provider_config.with_region(Some(region.clone()));
        }

        let profile_provider = ProfileFileCredentialsProvider::builder()
            .profile_files(profile_files)
            .profile_name(&self.profile)
            .configure(&provider_config)
            .build();

        profile_provider.provide_credentials().await
    }
}

impl ProvideCredentials for RefreshingFileCredentialsProvider {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(async move {
            // First, check if we need to refresh (read lock)
            let needs_refresh = {
                let state = self.state.read().await;
                state.needs_refresh(self.refresh_interval)
            };

            if needs_refresh {
                // Acquire write lock and refresh
                let mut state = self.state.write().await;

                // Double-check after acquiring write lock (another task might have refreshed)
                if state.needs_refresh(self.refresh_interval) {
                    tracing::debug!(
                        credentials_file = %self.credentials_file.display(),
                        profile = %self.profile,
                        "Refreshing AWS credentials from file"
                    );

                    match self.load_credentials().await {
                        Ok(credentials) => {
                            state.cached_credentials = Some(credentials.clone());
                            state.last_refresh = Some(Instant::now());
                            return Ok(credentials);
                        }
                        Err(e) => {
                            // If we have cached credentials and they're not expired,
                            // return them instead of failing
                            if let Some(cached) = &state.cached_credentials {
                                if !ProviderState::is_expired(cached) {
                                    tracing::warn!(
                                        error = %e,
                                        "Failed to refresh credentials, using cached credentials"
                                    );
                                    return Ok(cached.clone());
                                }
                            }
                            return Err(e);
                        }
                    }
                }
            }

            // Return cached credentials
            let state = self.state.read().await;
            state
                .cached_credentials
                .clone()
                .ok_or_else(|| provider::error::CredentialsError::not_loaded("no credentials cached"))
        })
    }
}

/// Builder for `RefreshingFileCredentialsProvider`.
#[derive(Debug, Default)]
pub struct RefreshingFileCredentialsProviderBuilder {
    credentials_file: Option<PathBuf>,
    profile: Option<String>,
    region: Option<Region>,
    http_client: Option<SharedHttpClient>,
    refresh_interval: Option<Duration>,
}

impl RefreshingFileCredentialsProviderBuilder {
    /// Set the path to the credentials file.
    pub fn credentials_file(mut self, path: impl Into<PathBuf>) -> Self {
        self.credentials_file = Some(path.into());
        self
    }

    /// Set the profile name to use.
    pub fn profile(mut self, profile: impl Into<String>) -> Self {
        self.profile = Some(profile.into());
        self
    }

    /// Set the AWS region for STS calls.
    pub fn region(mut self, region: Region) -> Self {
        self.region = Some(region);
        self
    }

    /// Set the HTTP client.
    pub fn http_client(mut self, client: SharedHttpClient) -> Self {
        self.http_client = Some(client);
        self
    }

    /// Set the refresh interval.
    ///
    /// The interval must be at least 30 seconds. Smaller values will be
    /// clamped to the minimum.
    pub fn refresh_interval(mut self, interval: Duration) -> Self {
        self.refresh_interval = Some(interval);
        self
    }

    /// Build the provider.
    ///
    /// # Panics
    ///
    /// Panics if `credentials_file` or `http_client` are not set.
    pub fn build(self) -> RefreshingFileCredentialsProvider {
        RefreshingFileCredentialsProvider::new(
            self.credentials_file
                .expect("credentials_file is required"),
            self.profile.unwrap_or_else(|| "default".to_string()),
            self.region,
            self.http_client.expect("http_client is required"),
            self.refresh_interval.unwrap_or(DEFAULT_REFRESH_INTERVAL),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn create_test_credentials_file(access_key: &str, secret_key: &str) -> NamedTempFile {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(
            file,
            r#"[default]
aws_access_key_id = {}
aws_secret_access_key = {}"#,
            access_key, secret_key
        )
        .unwrap();
        file
    }

    #[test]
    fn test_provider_state_needs_refresh() {
        let state = ProviderState::new();
        assert!(state.needs_refresh(Duration::from_secs(300)));
    }

    #[test]
    fn test_provider_state_with_cached_credentials() {
        let mut state = ProviderState::new();
        state.cached_credentials = Some(Credentials::new(
            "AKID",
            "SECRET",
            None,
            None, // No expiry
            "test",
        ));
        state.last_refresh = Some(Instant::now());

        // Should not need refresh immediately
        assert!(!state.needs_refresh(Duration::from_secs(300)));
    }

    #[test]
    fn test_provider_state_expired_refresh_interval() {
        let mut state = ProviderState::new();
        state.cached_credentials = Some(Credentials::new(
            "AKID",
            "SECRET",
            None,
            None,
            "test",
        ));
        // Set last refresh to a long time ago
        state.last_refresh = Some(Instant::now() - Duration::from_secs(600));

        // Should need refresh after interval passed
        assert!(state.needs_refresh(Duration::from_secs(300)));
    }

    #[test]
    fn test_refresh_interval_minimum() {
        // Test that refresh interval is clamped to minimum
        let provider = RefreshingFileCredentialsProvider::new(
            PathBuf::from("/tmp/creds"),
            "default".to_string(),
            None,
            aws_smithy_runtime::client::http::test_util::NeverClient::new().into_shared(),
            Duration::from_secs(1), // Too small
        );

        assert_eq!(provider.refresh_interval, MIN_REFRESH_INTERVAL);
    }
}

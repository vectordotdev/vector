//! Test harness for `vector top` integration tests
//!
//! Provides TestHarness for managing Vector process lifecycle
//! and helper functions for testing.

use std::fs::{OpenOptions, create_dir};
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::path::PathBuf;
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use assert_cmd::prelude::*;
use indoc::formatdoc;
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tokio::time::sleep;
use vector::test_util::{temp_dir, temp_file};
use vector_lib::api_client::{Client, gql::ComponentsQueryExt};

// Constants
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(2);
pub const EVENT_PROCESSING_TIMEOUT: Duration = Duration::from_secs(30);

/// Test harness for Vector instances with API enabled
///
/// Manages Vector process lifecycle, API client, and provides
/// helper methods for common test operations.
pub struct TestHarness {
    vector: Child,
    client: Client,
    api_port: u16,
    config_path: PathBuf,
    watch_mode: bool,
}

impl TestHarness {
    /// Spawns Vector with automatic port selection and retry on port conflicts
    ///
    /// Retries up to 3 times if port conflicts occur. Fails immediately on other errors.
    pub async fn new(pipeline_config: &str) -> Result<Self, String> {
        Self::new_internal(pipeline_config, false).await
    }

    /// Spawns Vector with watch mode enabled (-w flag)
    ///
    /// Watch mode automatically reloads config when the file changes.
    pub async fn new_with_watch_mode(pipeline_config: &str) -> Result<Self, String> {
        Self::new_internal(pipeline_config, true).await
    }

    async fn new_internal(pipeline_config: &str, watch_mode: bool) -> Result<Self, String> {
        const MAX_RETRIES: u32 = 3;
        const RETRY_DELAY: Duration = Duration::from_millis(500);

        for _attempt in 1..=MAX_RETRIES {
            let api_port = find_available_port();

            match Self::with_port_and_flags(pipeline_config, api_port, watch_mode).await {
                Ok(runner) => {
                    return Ok(runner);
                }
                Err(e) if is_port_conflict_error(&e) => {
                    sleep(RETRY_DELAY).await;
                    continue;
                }
                Err(e) => {
                    // Non-port-conflict error - fail immediately
                    return Err(e);
                }
            }
        }

        Err(format!(
            "Failed to start Vector after {MAX_RETRIES} attempts due to port conflicts"
        ))
    }

    async fn with_port_and_flags(
        pipeline_config: &str,
        api_port: u16,
        watch_mode: bool,
    ) -> Result<Self, String> {
        let config = formatdoc! {"
            api:
              enabled: true
              address: \"127.0.0.1:{api_port}\"

            {pipeline_config}
        "};

        let config_path = create_config_file(&config);
        let data_dir = create_data_directory();

        let mut cmd =
            Command::cargo_bin("vector").map_err(|e| format!("Failed to get cargo bin: {e}"))?;

        cmd.arg("-c").arg(&config_path);

        if watch_mode {
            cmd.arg("-w");
        }

        cmd.env("VECTOR_DATA_DIR", &data_dir);

        let mut vector = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn vector: {e}"))?;

        let client = Client::new(
            format!("http://127.0.0.1:{api_port}/graphql")
                .parse()
                .map_err(|e| format!("Invalid URL: {e}"))?,
        );

        // Wait for Vector startup with crash detection
        wait_for_startup(&mut vector, &client, STARTUP_TIMEOUT).await?;

        Ok(Self {
            vector,
            client,
            api_port,
            config_path,
            watch_mode,
        })
    }

    /// Queries all components from the GraphQL API
    pub async fn query_components(
        &self,
    ) -> Result<vector_lib::api_client::gql::components_query::ResponseData, String> {
        self.client
            .components_query(100)
            .await
            .map_err(|e| format!("Query failed: {e}"))?
            .data
            .ok_or_else(|| "No data in response".to_string())
    }

    /// Queries component IDs from the GraphQL API
    pub async fn query_component_ids(&self) -> Result<Vec<String>, String> {
        let data = self.query_components().await?;
        Ok(data
            .components
            .edges
            .iter()
            .map(|e| e.node.component_id.clone())
            .collect())
    }

    /// Waits for a component to process at least the expected number of events
    pub async fn wait_for_events(
        &self,
        component_id: &str,
        expected_events: i64,
    ) -> Result<i64, String> {
        wait_for_component_events(
            &self.client,
            component_id,
            expected_events,
            EVENT_PROCESSING_TIMEOUT,
        )
        .await
    }

    /// Reloads Vector configuration by sending SIGHUP
    ///
    /// Polls Vector to detect crashes early and succeed fast when reload completes.
    /// Waits until the expected component IDs are present in the topology.
    ///
    /// # Arguments
    /// * `new_pipeline_config` - The new pipeline configuration (without the API section)
    /// * `expected_component_ids` - Component IDs that must be present after reload
    pub async fn reload_with_config(
        &mut self,
        new_pipeline_config: &str,
        expected_component_ids: &[&str],
    ) -> Result<(), String> {
        let new_config = formatdoc! {"
            api:
              enabled: true
              address: \"127.0.0.1:{port}\"

            {new_pipeline_config}
        ", port = self.api_port};

        overwrite_config_file(&self.config_path, &new_config);

        // Send SIGHUP only if not in watch mode (watch mode auto-reloads on file change)
        if !self.watch_mode {
            kill(Pid::from_raw(self.vector.id() as i32), Signal::SIGHUP)
                .map_err(|e| format!("Failed to send SIGHUP: {e}"))?;
        }

        // Poll for reload completion with crash detection
        wait_for_topology_match(&mut self.vector, &self.client, expected_component_ids).await
    }

    /// Checks if Vector is still running
    pub fn check_running(&mut self) -> bool {
        self.vector.try_wait().unwrap().is_none()
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        // Send SIGTERM for graceful shutdown
        kill(Pid::from_raw(self.vector.id() as i32), Signal::SIGTERM).ok();

        // Wait for process to exit (without consuming self.vector)
        self.vector.wait().ok();
    }
}

/// Finds an available port by binding to port 0 and getting the OS-assigned port
///
/// Note: There's a small race condition between releasing the port (when TcpListener
/// is dropped) and Vector binding to it. In practice this is rare, but TestHarness::new()
/// handles this by retrying with a new port if Vector fails to start.
fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to port 0")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

/// Creates a temporary file with the given content
pub fn create_config_file(config: &str) -> PathBuf {
    let mut path = temp_file();
    // Add .yaml extension so Vector recognizes it as YAML
    path.set_extension("yaml");

    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&path)
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.sync_all().unwrap();

    path
}

/// Overwrites an existing config file with new content
pub fn overwrite_config_file(path: &PathBuf, config: &str) {
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.sync_all().unwrap();
}

/// Creates a temporary directory for Vector's data_dir
pub fn create_data_directory() -> PathBuf {
    let path = temp_dir();
    create_dir(&path).unwrap();
    path
}

/// Waits for Vector startup: polls for process health and API readiness
///
/// Fails fast if Vector crashes, succeeds fast if API becomes ready quickly.
pub async fn wait_for_startup(
    vector: &mut Child,
    client: &Client,
    timeout: Duration,
) -> Result<(), String> {
    let start = Instant::now();

    loop {
        // Check if Vector crashed
        if let Ok(Some(status)) = vector.try_wait() {
            let stderr = read_stderr(vector);
            return if status.success() {
                Err(
                    "Vector exited unexpectedly with success status (should stay running)"
                        .to_string(),
                )
            } else {
                Err(format!(
                    "Vector failed to start with status {status}\nStderr: {stderr}"
                ))
            };
        }

        // Check if API is ready
        if client.healthcheck().await.is_ok() {
            return Ok(());
        }

        // Check timeout
        if start.elapsed() >= timeout {
            return Err(format!(
                "Vector API did not become ready within {timeout:?}"
            ));
        }

        sleep(Duration::from_millis(100)).await;
    }
}

/// Reads stderr from a child process (non-blocking)
fn read_stderr(child: &mut Child) -> String {
    child
        .stderr
        .take()
        .map(|stderr| {
            let reader = BufReader::new(stderr);
            reader
                .lines()
                .map_while(Result::ok)
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default()
}

/// Checks if an error message indicates a port conflict
///
/// Looks for stable error indicators that don't depend on locale:
/// - "AddrInUse" (Rust's io::ErrorKind Debug representation)
/// - "EADDRINUSE" (Unix errno name)
fn is_port_conflict_error(error_message: &str) -> bool {
    error_message.contains("AddrInUse") || error_message.contains("EADDRINUSE")
}

/// Waits for topology match while checking for process crashes
///
/// Combines topology polling with crash detection for reload scenarios.
pub async fn wait_for_topology_match(
    vector: &mut Child,
    client: &Client,
    expected_component_ids: &[&str],
) -> Result<(), String> {
    let start = Instant::now();

    loop {
        // Check if Vector crashed
        if let Ok(Some(status)) = vector.try_wait() {
            return Err(format!("Vector crashed during reload with status {status}"));
        }

        // Query components to see if topology matches
        if let Ok(result) = client.components_query(100).await {
            if let Some(data) = result.data {
                let mut current_ids: Vec<&str> = data
                    .components
                    .edges
                    .iter()
                    .map(|e| e.node.component_id.as_str())
                    .collect();
                current_ids.sort_unstable();

                let mut expected_sorted = expected_component_ids.to_vec();
                expected_sorted.sort_unstable();

                if current_ids == expected_sorted {
                    return Ok(());
                }
            }
        }

        // Check timeout
        if start.elapsed() >= STARTUP_TIMEOUT {
            return Err(format!(
                "Topology did not match expected components within {STARTUP_TIMEOUT:?}"
            ));
        }

        sleep(Duration::from_millis(100)).await;
    }
}

/// Waits for a component to process the expected number of events
///
/// Polls the GraphQL API until the component's sent_events_total
/// reaches or exceeds the expected count.
async fn wait_for_component_events(
    client: &Client,
    component_id: &str,
    expected_events: i64,
    timeout: Duration,
) -> Result<i64, String> {
    let start = Instant::now();
    let mut last_count = 0;

    loop {
        if start.elapsed() > timeout {
            return Err(format!(
                "Timeout after {timeout:?}: component '{component_id}' only processed {last_count}/{expected_events} events"
            ));
        }

        let result = client
            .components_query(100)
            .await
            .map_err(|e| format!("Query failed: {e}"))?;

        let data = result.data.ok_or("No data in response")?;

        if let Some(component) = data
            .components
            .edges
            .iter()
            .find(|e| e.node.component_id == component_id)
        {
            let events = component.node.on.sent_events_total();

            if events != last_count {
                last_count = events;
            }

            if events >= expected_events {
                return Ok(events);
            }
        }

        sleep(Duration::from_millis(200)).await;
    }
}

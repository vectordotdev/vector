//! Shared test harness for Vector integration tests
//!
//! Provides `TestHarness` for managing Vector process lifecycle with API enabled.
//! Used by both `vector top` and `vector tap` integration tests.

use std::fs::{OpenOptions, create_dir};
use std::io::Write as _;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::time::{Duration, Instant};

use assert_cmd::prelude::*;
use nix::{
    sys::signal::{Signal, kill},
    unistd::Pid,
};
use tempfile::TempDir;
use tokio::time::sleep;
use vector_lib::api_client::Client;

// Constants
pub const STARTUP_TIMEOUT: Duration = Duration::from_secs(10);
pub const EVENT_PROCESSING_TIMEOUT: Duration = Duration::from_secs(30);

/// Test harness for Vector instances with API enabled
///
/// Manages Vector process lifecycle, API client, and provides
/// helper methods for common test operations.
pub struct TestHarness {
    vector: Child,
    api_client: Client,
    api_port: u16,
    config_path: PathBuf,
    watch_mode: bool,
    directory: TempDir,
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
                Err(e) if e.contains("Address already in use") => {
                    sleep(RETRY_DELAY).await;
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
        let directory = tempfile::tempdir().expect("Failed to create test directory");
        let data_dir = directory.path().join("data");
        create_dir(&data_dir).expect("Failed to create data directory");
        let config_path = directory.path().join("vector.yaml");
        let config = harness_config(pipeline_config, api_port, &data_dir);
        overwrite_config_file(&config_path, &config);

        let mut cmd =
            Command::cargo_bin("vector").map_err(|e| format!("Failed to get cargo bin: {e}"))?;

        cmd.arg("-c")
            .arg(&config_path)
            .arg("--graceful-shutdown-limit-secs")
            .arg("1");

        if watch_mode {
            cmd.arg("-w");
        }

        let vector = cmd
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()
            .map_err(|e| format!("Failed to spawn vector: {e}"))?;

        let url = format!("http://127.0.0.1:{api_port}");
        let api_client = Client::new(url.parse().expect("valid URL"));

        // Own the child before waiting for startup so failures also stop it before
        // the temporary directory is removed.
        let mut harness = Self {
            vector,
            api_client,
            api_port,
            config_path,
            watch_mode,
            directory,
        };

        // Wait for Vector startup with crash detection
        // This will repeatedly try to connect until Vector is ready
        wait_for_startup(
            &mut harness.vector,
            &mut harness.api_client,
            STARTUP_TIMEOUT,
        )
        .await?;

        Ok(harness)
    }

    /// Returns mutable reference to the API client
    pub fn api_client(&mut self) -> &mut Client {
        &mut self.api_client
    }

    /// Returns the TCP port the API server is bound to
    pub fn api_port(&self) -> u16 {
        self.api_port
    }

    /// Reloads Vector configuration by sending SIGHUP or using watch mode
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
        let new_config = harness_config(
            new_pipeline_config,
            self.api_port,
            &self.directory.path().join("data"),
        );

        overwrite_config_file(&self.config_path, &new_config);

        // Send SIGHUP only if not in watch mode (watch mode auto-reloads on file change)
        if !self.watch_mode {
            kill(
                Pid::from_raw(self.vector.id().cast_signed()),
                Signal::SIGHUP,
            )
            .map_err(|e| format!("Failed to send SIGHUP: {e}"))?;
        }

        // Give Vector a moment to process the reload signal before polling
        sleep(Duration::from_millis(500)).await;

        // Poll for reload completion with crash detection
        wait_for_topology_match(
            &mut self.vector,
            &mut self.api_client,
            expected_component_ids,
        )
        .await
    }

    /// Checks if Vector is still running
    pub fn check_running(&mut self) -> bool {
        self.vector.try_wait().unwrap().is_none()
    }
}

impl Drop for TestHarness {
    fn drop(&mut self) {
        // Send SIGTERM for graceful shutdown
        if self.vector.try_wait().ok().flatten().is_none() {
            kill(
                Pid::from_raw(self.vector.id().cast_signed()),
                Signal::SIGTERM,
            )
            .ok();
        }

        // Wait for process to exit
        self.vector.wait().ok();
    }
}

/// Finds an available port by binding to port 0 and getting the OS-assigned port
///
/// Note: There's a small race condition between releasing the port (when `TcpListener`
/// is dropped) and Vector binding to it. In practice this is rare, but `TestHarness::new()`
/// handles this by retrying with a new port if Vector fails to start.
fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to port 0")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

fn harness_config(pipeline_config: &str, api_port: u16, data_dir: &Path) -> String {
    let mut config: serde_json::Value =
        serde_yaml::from_str(pipeline_config).expect("Invalid pipeline configuration");
    config["data_dir"] = serde_json::json!(data_dir);
    config["api"] = serde_json::json!({
        "enabled": true,
        "address": format!("127.0.0.1:{api_port}"),
    });
    serde_yaml::to_string(&config).expect("Failed to serialize configuration")
}

/// Overwrites an existing config file with new content
pub fn overwrite_config_file(path: &Path, config: &str) {
    let mut file = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .unwrap();

    file.write_all(config.as_bytes()).unwrap();
    file.sync_all().unwrap();
}

/// Waits for Vector startup: polls for process health and API readiness
///
/// Fails fast if Vector crashes, succeeds fast if API becomes ready quickly.
pub async fn wait_for_startup(
    vector: &mut Child,
    client: &mut Client,
    timeout: Duration,
) -> Result<(), String> {
    let start = Instant::now();
    // This opens no connection; the health RPC below fails until the API is ready.
    client
        .connect()
        .map_err(|e| format!("Failed to set up the API client: {e}"))?;

    loop {
        // Check if Vector crashed
        if let Ok(Some(status)) = vector.try_wait() {
            return if status.success() {
                Err(
                    "Vector exited unexpectedly with success status (should stay running)"
                        .to_string(),
                )
            } else {
                Err(format!("Vector failed to start with status {status}"))
            };
        }

        // Check if API is ready
        if client.health().await.is_ok() {
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

/// Waits for topology match while checking for process crashes
///
/// Combines topology polling with crash detection for reload scenarios.
pub async fn wait_for_topology_match(
    vector: &mut Child,
    client: &mut Client,
    expected_component_ids: &[&str],
) -> Result<(), String> {
    let start = Instant::now();
    let mut last_components: Vec<String> = Vec::new();

    loop {
        // Check if Vector crashed
        if let Ok(Some(status)) = vector.try_wait() {
            return Err(format!("Vector crashed during reload with status {status}"));
        }

        // Query components to see if topology matches
        if let Ok(response) = client.get_components(100).await {
            let mut current_ids: Vec<String> = response
                .components
                .iter()
                .map(|c| c.component_id.clone())
                .collect();
            current_ids.sort_unstable();

            let mut expected_sorted: Vec<String> = expected_component_ids
                .iter()
                .map(ToString::to_string)
                .collect();
            expected_sorted.sort_unstable();

            // Track last seen components for better error reporting
            if current_ids != last_components {
                last_components.clone_from(&current_ids);
            }

            if current_ids == expected_sorted {
                return Ok(());
            }
        }

        // Check timeout
        if start.elapsed() >= STARTUP_TIMEOUT {
            return Err(format!(
                "Topology did not match expected components within {STARTUP_TIMEOUT:?}. Last seen: {last_components:?}, expected: {expected_component_ids:?}"
            ));
        }

        sleep(Duration::from_millis(100)).await;
    }
}

/// Waits for a component to process the expected number of events
///
/// Polls the gRPC API until the component's `sent_events_total`
/// reaches or exceeds the expected count.
pub async fn wait_for_component_events(
    client: &mut Client,
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

        let response = client
            .get_components(100)
            .await
            .map_err(|e| format!("Query failed: {e}"))?;

        if let Some(component) = response
            .components
            .iter()
            .find(|c| c.component_id == component_id)
        {
            let events = component
                .metrics
                .as_ref()
                .and_then(|m| m.sent_events_total)
                .unwrap_or(0);

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

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::TestHarness;

    #[tokio::test]
    async fn disk_buffers_are_isolated_and_keep_their_directory_across_reload() {
        let pipeline = indoc! {"
            sources:
              demo_source:
                type: demo_logs
                format: json
                interval: 0.01
            sinks:
              out:
                type: blackhole
                inputs: [demo_source]
                buffer:
                  type: disk
                  max_size: 268435488
        "};

        for watch_mode in [false, true] {
            let mut first = TestHarness::new_internal(pipeline, watch_mode)
                .await
                .unwrap();
            let second = TestHarness::new(pipeline).await.unwrap();
            let first_directory = first.directory.path().to_path_buf();
            let second_directory = second.directory.path().to_path_buf();
            let ledger = "data/buffer/v2/out/buffer.db";

            assert_ne!(first_directory, second_directory);
            assert!(first_directory.join(ledger).is_file());
            assert!(second_directory.join(ledger).is_file());

            first
                .reload_with_config(
                    &pipeline.replace("demo_source", "replacement_source"),
                    &["replacement_source", "out"],
                )
                .await
                .unwrap();
            assert!(first_directory.join(ledger).is_file());

            drop(first);
            assert!(!first_directory.exists());
            assert!(second_directory.join(ledger).is_file());
            drop(second);
            assert!(!second_directory.exists());
        }
    }
}

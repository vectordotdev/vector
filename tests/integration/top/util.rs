//! Utilities for `vector top` integration tests
//!
//! All helpers are self-contained in this module to avoid
//! polluting the global test utilities.

use std::process::{Command, Child};
use std::time::{Duration, Instant};
use std::fs::{OpenOptions, create_dir};
use std::io::Write;
use std::path::PathBuf;
use std::net::TcpListener;

use assert_cmd::prelude::*;
use indoc::formatdoc;
use tokio::time::sleep;
use nix::{sys::signal::{Signal, kill}, unistd::Pid};
use vector::test_util::{temp_dir, temp_file};
use vector_lib::api_client::{Client, gql::ComponentsQueryExt};

// Re-export types needed by tests
pub use vector_lib::api_client::gql::components_query;

/// Finds an available port by binding to port 0 and getting the OS-assigned port
///
/// Note: There's a small race condition between releasing the port (when TcpListener
/// is dropped) and Vector binding to it. In practice this is rare, but tests handle
/// this by retrying with a new port if Vector fails to start.
pub fn find_available_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("Failed to bind to port 0")
        .local_addr()
        .expect("Failed to get local address")
        .port()
}

// Constants
pub const STARTUP_TIME: Duration = Duration::from_secs(2);
pub const API_READY_TIMEOUT: Duration = Duration::from_secs(10);
pub const EVENT_PROCESSING_TIMEOUT: Duration = Duration::from_secs(30);
pub const RELOAD_TIME: Duration = Duration::from_secs(3);

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
    file.flush().unwrap();
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
    file.flush().unwrap();
    file.sync_all().unwrap();
}

/// Creates a temporary directory for Vector's data_dir
pub fn create_data_directory() -> PathBuf {
    let path = temp_dir();
    create_dir(&path).unwrap();
    path
}

/// Spawns a Vector process with the given config
pub fn spawn_vector_with_api(config: &str) -> Child {
    let mut cmd = Command::cargo_bin("vector").unwrap();
    cmd.arg("-c")
        .arg(create_config_file(config))
        .env("VECTOR_DATA_DIR", create_data_directory());

    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("Failed to start vector")
}

/// Spawns Vector with automatic port selection and retry on port conflicts
///
/// Handles the race condition in `find_available_port()` by retrying with a new
/// port if Vector fails to start due to "Address already in use" errors.
/// Other errors cause immediate failure.
///
/// Returns (Child process, Client, successfully bound port)
#[allow(dead_code)]
pub async fn spawn_vector_with_retry(pipeline_config: &str) -> (Child, Client, u16) {
    const MAX_RETRIES: u32 = 3;
    const RETRY_DELAY: Duration = Duration::from_millis(500);

    for attempt in 1..=MAX_RETRIES {
        let api_port = find_available_port();
        let config = build_config_with_api(api_port, pipeline_config);
        let mut vector = spawn_vector_with_api(&config);

        sleep(STARTUP_TIME).await;

        // Check if Vector is still running
        if let Ok(Some(status)) = vector.try_wait() {
            // Vector exited - get stderr to check for port conflict
            let output = vector.wait_with_output().unwrap_or_else(|e| {
                panic!("Failed to get Vector output: {}", e);
            });

            let stderr = String::from_utf8_lossy(&output.stderr);

            // Only retry if it's a port conflict
            if stderr.contains("Address already in use") {
                eprintln!(
                    "Attempt {}/{}: Port {} already in use, retrying with new port...",
                    attempt, MAX_RETRIES, api_port
                );
                sleep(RETRY_DELAY).await;
                continue;
            } else {
                // Different error - fail immediately
                eprintln!("=== Vector stderr ===\n{}", stderr);
                panic!(
                    "Vector exited early with status {} (not a port conflict). See stderr above.",
                    status
                );
            }
        }

        let client = create_client(api_port);

        // Try to connect to the API
        match wait_for_api_ready(&client, API_READY_TIMEOUT).await {
            Ok(()) => {
                eprintln!("Successfully started Vector on port {}", api_port);
                return (vector, client, api_port);
            }
            Err(e) => {
                // API not ready - could be port conflict or other issue
                // Kill the Vector instance to check stderr
                let _ = kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM);
                let output = vector.wait_with_output().unwrap_or_else(|err| {
                    panic!("Failed to get Vector output: {}", err);
                });

                let stderr = String::from_utf8_lossy(&output.stderr);

                // Only retry if it's a port conflict
                if stderr.contains("Address already in use") {
                    eprintln!(
                        "Attempt {}/{}: Port {} already in use (API check failed: {}), retrying...",
                        attempt, MAX_RETRIES, api_port, e
                    );
                    sleep(RETRY_DELAY).await;
                    continue;
                } else {
                    // Different error - fail immediately
                    eprintln!("=== Vector stderr ===\n{}", stderr);
                    panic!(
                        "API did not become ready ({}). Not a port conflict. See stderr above.",
                        e
                    );
                }
            }
        }
    }

    panic!("Failed to start Vector after {} attempts due to port conflicts", MAX_RETRIES);
}

/// Waits for the Vector API to become ready
pub async fn wait_for_api_ready(
    client: &Client,
    timeout: Duration,
) -> Result<(), String> {
    let start = Instant::now();

    while start.elapsed() < timeout {
        if client.healthcheck().await.is_ok() {
            return Ok(());
        }
        sleep(Duration::from_millis(100)).await;
    }

    Err(format!("API did not become ready within {:?}", timeout))
}

/// Waits for a component to process the expected number of events
///
/// Polls the GraphQL API until the component's sent_events_total
/// reaches or exceeds the expected count.
pub async fn wait_for_component_events(
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
                "Timeout after {:?}: component '{}' only processed {}/{} events",
                timeout, component_id, last_count, expected_events
            ));
        }

        let result = client.components_query(100).await
            .map_err(|e| format!("Query failed: {}", e))?;

        let data = result.data.ok_or("No data in response")?;

        if let Some(component) = data.components.edges.iter()
            .find(|e| e.node.component_id == component_id)
        {
            let events = component.node.on.sent_events_total();

            if events != last_count {
                eprintln!(
                    "[{:>6.1}s] Component '{}' has {}/{} events",
                    start.elapsed().as_secs_f32(),
                    component_id,
                    events,
                    expected_events
                );
                last_count = events;
            }

            if events >= expected_events {
                return Ok(events);
            }
        }

        sleep(Duration::from_millis(200)).await;
    }
}

/// Creates a Vector API client for the given port
pub fn create_client(api_port: u16) -> Client {
    Client::new(
        format!("http://127.0.0.1:{api_port}/graphql")
            .parse()
            .unwrap()
    )
}

/// Cleanly shuts down Vector and verifies successful exit
pub fn cleanup_vector(vector: Child) {
    // Send SIGTERM for graceful shutdown
    kill(Pid::from_raw(vector.id() as i32), Signal::SIGTERM).unwrap();

    // Wait for process to exit
    let output = vector.wait_with_output().unwrap();

    // Verify successful exit
    if !output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        eprintln!("=== Vector stdout ===\n{}", stdout);
        eprintln!("=== Vector stderr ===\n{}", stderr);
        panic!("Vector didn't exit successfully. Status: {}", output.status);
    }
}

/// Helper to build a basic Vector config with API enabled
pub fn build_config_with_api(api_port: u16, pipeline: &str) -> String {
    formatdoc! {"
        api:
          enabled: true
          address: \"127.0.0.1:{api_port}\"

        {pipeline}
    "}
}

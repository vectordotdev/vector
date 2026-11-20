//! Runs a test's `lifecycle.before_validation` steps after its compose environment starts, so
//! tests read complete captures instead of racing the services that write them.

use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::Compose;
use crate::{
    app::{self, CommandExt as _},
    testing::{
        config::{LifecycleStep, RunService, StopGroup, WaitForSuccess},
        docker::docker_command,
    },
    utils::environment::{Environment, extract_present},
};

const POLL_INTERVAL: Duration = Duration::from_millis(500);

/// Records that every step succeeded, so `vdev e2e test` never validates an environment whose
/// lifecycle failed or did not run.
pub(super) fn completion_marker(project_name: &str) -> PathBuf {
    [
        app::path(),
        "target",
        "vdev-lifecycle",
        &format!("{project_name}.complete"),
    ]
    .iter()
    .collect()
}

pub(super) fn run(
    compose: &Compose,
    environment: &Environment,
    project_name: &str,
    steps: &[LifecycleStep],
) -> Result<()> {
    let steps_runner = Steps {
        compose,
        environment,
        project_name,
    };
    for (index, step) in steps.iter().enumerate() {
        info!("Lifecycle step {}: {}", index + 1, describe(step));
        match step {
            LifecycleStep::WaitForSuccess(wait) => steps_runner.wait_for_success(wait),
            LifecycleStep::SleepSecs(seconds) => {
                thread::sleep(Duration::from_secs(*seconds));
                Ok(())
            }
            LifecycleStep::Stop(group) => steps_runner.stop(group),
            LifecycleStep::Run(service) => steps_runner.run_service(service),
        }
        .with_context(|| format!("lifecycle step {} ({}) failed", index + 1, describe(step)))?;
    }

    let marker = completion_marker(project_name);
    if let Some(parent) = marker.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }
    fs::write(&marker, "").with_context(|| format!("failed to write {}", marker.display()))
}

fn describe(step: &LifecycleStep) -> String {
    match step {
        LifecycleStep::WaitForSuccess(wait) => format!("wait for {} to succeed", wait.service),
        LifecycleStep::SleepSecs(seconds) => format!("sleep {seconds}s"),
        LifecycleStep::Stop(group) => {
            let targets = group.services.iter().chain(&group.labels);
            format!("stop {}", targets.cloned().collect::<Vec<_>>().join(", "))
        }
        LifecycleStep::Run(service) => format!("run {}", service.service),
    }
}

/// The subset of `docker inspect`'s `State` the steps check.
#[derive(Debug, Deserialize)]
struct ContainerState {
    #[serde(rename = "Status")]
    status: String,
    #[serde(rename = "ExitCode")]
    exit_code: i64,
    #[serde(rename = "OOMKilled")]
    oom_killed: bool,
}

impl ContainerState {
    fn has_exited(&self) -> bool {
        matches!(self.status.as_str(), "exited" | "dead")
    }

    fn failure(&self) -> Option<String> {
        if self.oom_killed {
            Some("was OOM-killed".into())
        } else if self.exit_code != 0 {
            Some(format!("exited with code {}", self.exit_code))
        } else {
            None
        }
    }
}

fn service_label(service: &str) -> String {
    format!("com.docker.compose.service={service}")
}

struct Steps<'a> {
    compose: &'a Compose,
    environment: &'a Environment,
    project_name: &'a str,
}

impl Steps<'_> {
    fn compose_command(&self) -> std::process::Command {
        let mut command = self.compose.command(self.project_name);
        command.envs(extract_present(self.environment));
        command
    }

    /// This project's containers with `label`, excluding `compose run` one-offs.
    fn containers(&self, label: &str) -> Result<Vec<String>> {
        let output = docker_command(["ps", "--all", "--quiet"])
            .args([
                "--filter",
                &format!("label=com.docker.compose.project={}", self.project_name),
            ])
            .args(["--filter", "label=com.docker.compose.oneoff=False"])
            .args(["--filter", &format!("label={label}")])
            .check_output()?;
        Ok(output.split_whitespace().map(str::to_owned).collect())
    }

    fn inspect(container: &str) -> Result<(String, ContainerState)> {
        let output = docker_command([
            "inspect",
            "--format",
            "{{.Name}}\t{{json .State}}",
            container,
        ])
        .check_output()?;
        let Some((name, state)) = output.trim().split_once('\t') else {
            bail!("unexpected docker inspect output: {output:?}");
        };
        let state = serde_json::from_str(state)
            .with_context(|| format!("failed to parse state of {name}: {state}"))?;
        Ok((name.trim_start_matches('/').to_owned(), state))
    }

    fn wait_for_success(&self, wait: &WaitForSuccess) -> Result<()> {
        let deadline = Instant::now() + Duration::from_secs(wait.timeout_secs);
        loop {
            let containers = self.containers(&service_label(&wait.service))?;
            let [container] = containers.as_slice() else {
                bail!(
                    "expected one {} container, found {}",
                    wait.service,
                    containers.len()
                );
            };
            let (name, state) = Self::inspect(container)?;
            if state.has_exited() {
                return match state.failure() {
                    Some(failure) => bail!("{name} {failure}"),
                    None => Ok(()),
                };
            }
            if Instant::now() >= deadline {
                bail!("{name} did not finish within {}s", wait.timeout_secs);
            }
            thread::sleep(POLL_INTERVAL);
        }
    }

    fn stop(&self, group: &StopGroup) -> Result<()> {
        let mut containers = BTreeSet::new();
        let services = group.services.iter().map(|service| service_label(service));
        for label in services.chain(group.labels.iter().cloned()) {
            containers.extend(self.containers(&label)?);
        }
        if containers.is_empty() {
            bail!("no containers matched");
        }

        // Stopping an already exited container succeeds, so one that crashed earlier is still
        // checked below.
        docker_command(["stop", "-t", &group.timeout_secs.to_string()])
            .args(&containers)
            .check_output()?;

        let mut failures = Vec::new();
        for container in &containers {
            let (name, state) = Self::inspect(container)?;
            if group.require_success
                && let Some(failure) = state.failure()
            {
                failures.push(format!("{name} {failure}"));
            }
            if !group.reject_log_messages.is_empty() {
                let output = docker_command(["logs", container]).output()?;
                // Otherwise the error text would be searched and the check silently skipped.
                if !output.status.success() {
                    bail!(
                        "failed to read logs of {name}: {}",
                        String::from_utf8_lossy(&output.stderr).trim()
                    );
                }
                let logs = [output.stdout, output.stderr].concat();
                let logs = String::from_utf8_lossy(&logs);
                for message in &group.reject_log_messages {
                    if logs.contains(message.as_str()) {
                        failures.push(format!("{name} logged {message:?}"));
                    }
                }
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            bail!("{}", failures.join("; "))
        }
    }

    fn run_service(&self, service: &RunService) -> Result<()> {
        let mut child = self
            .compose_command()
            .args(["run", "--rm", "--no-deps", &service.service])
            .spawn()
            .with_context(|| format!("failed to run {}", service.service))?;
        let deadline = Instant::now() + Duration::from_secs(service.timeout_secs);
        loop {
            if let Some(status) = child.try_wait()? {
                if status.success() {
                    return Ok(());
                }
                bail!("{} exited with {status}", service.service);
            }
            if Instant::now() >= deadline {
                // Compose `down` removes the run container during cleanup.
                child.kill().ok();
                bail!(
                    "{} did not finish within {}s",
                    service.service,
                    service.timeout_secs
                );
            }
            thread::sleep(POLL_INTERVAL);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(status: &str, exit_code: i64, oom_killed: bool) -> ContainerState {
        ContainerState {
            status: status.into(),
            exit_code,
            oom_killed,
        }
    }

    #[test]
    fn container_state_reports_failures() {
        assert!(state("exited", 0, false).failure().is_none());
        assert_eq!(
            state("exited", 3, false).failure().as_deref(),
            Some("exited with code 3")
        );
        assert_eq!(
            state("exited", 137, true).failure().as_deref(),
            Some("was OOM-killed")
        );
        assert!(state("dead", 0, false).has_exited());
        assert!(!state("running", 0, false).has_exited());
    }

    #[test]
    fn parses_docker_inspect_state() {
        let state: ContainerState = serde_json::from_str(
            r#"{"Status":"exited","Running":false,"ExitCode":3,"OOMKilled":false,"Pid":0}"#,
        )
        .unwrap();
        assert_eq!(state.failure().as_deref(), Some("exited with code 3"));
    }

    #[test]
    fn describes_steps() {
        assert_eq!(
            describe(&LifecycleStep::Stop(StopGroup {
                services: vec!["vector".into()],
                labels: vec!["producer=true".into()],
                timeout_secs: 1,
                require_success: true,
                reject_log_messages: vec![],
            })),
            "stop vector, producer=true"
        );
    }
}

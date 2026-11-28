use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use anyhow::{Context, Result, bail};
use serde::Deserialize;

use super::{
    config::{
        ComposeConfig, ComposeTestConfig, E2E_TESTS_DIR, INTEGRATION_TESTS_DIR, RustToolchainConfig,
    },
    runner::{
        ContainerTestRunner as _, IntegrationTestRunner, RunnerState, TARGET_PATH, TestRunner as _,
    },
};
use crate::{
    app::CommandExt as _,
    testing::{
        build::ALL_INTEGRATIONS_FEATURE_FLAG,
        docker::{CONTAINER_TOOL, DOCKER_SOCKET, docker_command},
    },
    utils::environment::{Environment, extract_present, rename_environment_keys},
};

const NETWORK_ENV_VAR: &str = "VECTOR_NETWORK";
const E2E_FEATURE_FLAG: &str = "all-e2e-tests";
/// Where the committed E2E image keeps Vector.
const E2E_VECTOR_BINARY: &str = "/usr/bin/vector";
/// Where the runner records the sha256 of [`E2E_VECTOR_BINARY`] after the build.
const E2E_VECTOR_DIGEST: &str = "/tmp/vector.sha256";
/// Records the sha256 of the Vector binary an E2E Vector image was committed with.
const E2E_VECTOR_BINARY_LABEL: &str = "vector.e2e.binary-sha256";

/// Check if a Docker image exists locally
fn docker_image_exists(image_name: &str) -> Result<bool> {
    let output =
        docker_command(["images", "--format", "{{.Repository}}:{{.Tag}}"]).check_output()?;
    Ok(output.lines().any(|line| line == image_name))
}

/// Image that the compose `vector` services of E2E tests run, committed from `runner_container`.
///
/// Naming it after the runner keeps suites with their own runners (and feature sets) from
/// overwriting each other's image when they run concurrently.
fn e2e_vector_image_name(runner_container: &str) -> String {
    format!("{runner_container}-e2e-vector:latest")
}

/// Runs right after the build, under the same lock: copy the shared `debug/vector` into the runner
/// and hash the copy. Another runner relinking the shared binary afterwards then can't change
/// what gets committed, and the digest always describes the committed binary.
fn e2e_snapshot_script() -> String {
    format!(
        "cp {TARGET_PATH}/debug/vector {E2E_VECTOR_BINARY} && \
         sha256sum {E2E_VECTOR_BINARY} > {E2E_VECTOR_DIGEST}"
    )
}

/// Extract the digest from `sha256sum` output (`<digest>  <path>`).
fn parse_sha256sum(output: &str) -> Option<&str> {
    output
        .split_whitespace()
        .next()
        .filter(|digest| digest.len() == 64 && digest.bytes().all(|b| b.is_ascii_hexdigit()))
}

/// The `--features` value for a suite: `shared_feature` (if any), then the suite's own features,
/// then `vendored`.
///
/// E2E suites keep their own features alongside `all-e2e-tests` because the same build produces
/// the Vector binary their compose services run, and some suites enable extra features (e.g.
/// `typed-trace`) that the aggregate feature does not include.
fn cargo_features(shared_feature: Option<&str>, suite_features: &[String]) -> String {
    let mut features: Vec<&str> = shared_feature.into_iter().collect();
    for feature in suite_features {
        if !features.contains(&feature.as_str()) {
            features.push(feature);
        }
    }
    features.push("vendored");
    features.join(",")
}

/// The parts of `docker image inspect` output needed to decide whether an image can be reused.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageInspect {
    id: String,
    #[serde(default)]
    config: ImageConfig,
    #[serde(default, rename = "RootFS")]
    root_fs: ImageRootFs,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageConfig {
    #[serde(default)]
    labels: Option<BTreeMap<String, String>>,
    // Runtime config that `docker commit` copies from the runner container. vdev creates that
    // container without overriding these, so they match the runner image's unless the runner
    // changed. `Cmd` and `WorkingDir` are left out because vdev sets them on the container.
    #[serde(default)]
    env: Option<Vec<String>>,
    #[serde(default)]
    entrypoint: Option<Vec<String>>,
    #[serde(default)]
    user: Option<String>,
}

impl ImageConfig {
    fn same_runtime_config(&self, other: &Self) -> bool {
        self.env == other.env && self.entrypoint == other.entrypoint && self.user == other.user
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageRootFs {
    #[serde(default)]
    layers: Vec<String>,
}

impl ImageInspect {
    /// Parse `docker image inspect` output for a single image.
    fn parse(output: &str) -> Result<Self> {
        let mut images: Vec<Self> =
            serde_json::from_str(output).context("Invalid docker image inspect output")?;
        match images.len() {
            1 => Ok(images.remove(0)),
            count => bail!("Expected one image from docker image inspect, got {count}"),
        }
    }

    fn label(&self, name: &str) -> Option<&str> {
        self.config.labels.as_ref()?.get(name).map(String::as_str)
    }

    /// Whether this image was committed from a container of `runner` holding a Vector binary
    /// with sha256 `binary_digest`.
    ///
    /// This compares filesystem layers and runtime config, not image IDs: rebuilding the runner
    /// image from cache yields the same layers but can yield a new ID (e.g. with the containerd
    /// image store). Config-only changes such as `ENV` keep the layers, hence the config check.
    fn is_committed_from(&self, runner: &ImageInspect, binary_digest: &str) -> bool {
        self.label(E2E_VECTOR_BINARY_LABEL) == Some(binary_digest)
            && self.root_fs.layers.len() == runner.root_fs.layers.len() + 1
            && self.root_fs.layers.starts_with(&runner.root_fs.layers)
            && self.config.same_runtime_config(&runner.config)
    }
}

fn inspect_image(image: &str) -> Result<ImageInspect> {
    ImageInspect::parse(&docker_command(["image", "inspect", image]).check_output()?)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ComposeTestKind {
    E2E,
    Integration,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ComposeTestLocalConfig {
    pub(crate) kind: ComposeTestKind,
    pub(crate) directory: &'static str,
    pub(crate) feature_flag: &'static str,
}

impl ComposeTestLocalConfig {
    /// Integration tests are located in the `tests/integration` dir,
    /// and are the full feature flag is `all-integration-tests`.
    pub(crate) fn integration() -> Self {
        Self {
            kind: ComposeTestKind::Integration,
            directory: INTEGRATION_TESTS_DIR,
            feature_flag: ALL_INTEGRATIONS_FEATURE_FLAG,
        }
    }

    /// E2E tests are located in the `tests/e2e` dir,
    /// and the full feature flag is `all-e2e-tests`.
    pub(crate) fn e2e() -> Self {
        Self {
            kind: ComposeTestKind::E2E,
            directory: E2E_TESTS_DIR,
            feature_flag: E2E_FEATURE_FLAG,
        }
    }
}

pub(crate) fn collect_compose_logs(
    local_config: ComposeTestLocalConfig,
    test_name: &str,
    environment: &str,
) -> Result<Option<ExitStatus>> {
    ComposeTest::generate(local_config, test_name, environment, 0, false)?.logs()
}

#[derive(Debug)]
pub(crate) struct ComposeTest {
    local_config: ComposeTestLocalConfig,
    test_name: String,
    environment: String,
    config: ComposeTestConfig,
    runner: IntegrationTestRunner,
    compose: Option<Compose>,
    env_config: Environment,
    retries: u8,
    coverage: bool,
}

impl ComposeTest {
    pub(crate) fn generate(
        local_config: ComposeTestLocalConfig,
        test_name: impl Into<String>,
        environment: impl Into<String>,
        retries: u8,
        coverage: bool,
    ) -> Result<ComposeTest> {
        let test_name: String = test_name.into();
        let environment = environment.into();
        let (test_dir, config) = ComposeTestConfig::load(local_config.directory, &test_name)?;
        let Some(mut env_config) = config.environments().get(&environment).cloned() else {
            bail!("Could not find environment named {environment:?}");
        };

        let network_name = format!("vector-integration-tests-{test_name}");
        let compose = Compose::new(test_dir, env_config.clone(), network_name.clone())?;

        // Auto-detect: If shared image exists, use it. Otherwise use per-test image.
        // Shared image: vector-test-runner-1.90:latest (compiled with all-integration-tests)
        // Per-test image: vector-test-runner-clickhouse-1.90:latest (compiled with specific features)
        let shared_image_name = format!(
            "vector-test-runner-{}:latest",
            RustToolchainConfig::rust_version()
        );
        let runner_name = if docker_image_exists(&shared_image_name).unwrap_or(false) {
            info!("Using shared runner image: {shared_image_name}");
            None
        } else {
            info!("Shared runner image not found, will build image for: {test_name}");
            Some(test_name.clone())
        };

        let runner = IntegrationTestRunner::new(
            runner_name,
            &config.runner,
            compose.is_some().then_some(network_name),
        )?;

        let vector_image = match local_config.kind {
            ComposeTestKind::E2E => e2e_vector_image_name(&runner.container_name()),
            ComposeTestKind::Integration => runner.image_name(),
        };
        env_config.insert("VECTOR_IMAGE".to_string(), Some(vector_image));

        let compose_test = ComposeTest {
            local_config,
            test_name,
            environment,
            config,
            runner,
            compose,
            env_config: rename_environment_keys(&env_config),
            retries,
            coverage,
        };
        trace!("Generated {compose_test:#?}");
        Ok(compose_test)
    }

    fn project_name(&self) -> String {
        // Docker Compose project names must consist only of lowercase alphanumeric characters,
        // hyphens, and underscores. Replace any dots with hyphens.
        let sanitized_env = self.environment.replace('.', "-");
        format!(
            "vector-{}-{}-{sanitized_env}",
            self.local_config.directory, self.test_name
        )
    }

    fn is_running(&self) -> Result<bool> {
        let Some(compose) = &self.compose else {
            return Ok(false);
        };

        let output = Command::new(CONTAINER_TOOL.clone())
            .args([
                "compose",
                "--project-name",
                &self.project_name(),
                "ps",
                "--format",
                "json",
                "--status",
                "running",
            ])
            .current_dir(&compose.test_dir)
            .envs(
                compose
                    .env
                    .iter()
                    .filter_map(|(k, v)| v.as_ref().map(|val| (k, val))),
            )
            .output()
            .with_context(|| "Failed to check if compose environment is running")?;

        // If stdout is empty or "[]", no containers are running
        Ok(!output.stdout.is_empty() && output.stdout != b"[]\n" && output.stdout != b"[]")
    }

    pub(crate) fn test(&self, extra_args: Vec<String>) -> Result<()> {
        let was_running = self.is_running()?;
        self.config.check_required()?;

        if !was_running {
            self.start()?;
        }

        let mut env_vars = self.config.env.clone();
        // Make sure the test runner has the same config environment vars as the services do.
        for (key, value) in self.env_config.clone() {
            env_vars.insert(key, value);
        }

        env_vars.insert("VECTOR_LOG".to_string(), Some("info".into()));
        if let Ok(val) = std::env::var("CARGO_TERM_COLOR") {
            env_vars.insert("CARGO_TERM_COLOR".to_string(), Some(val));
        }
        let mut args = self.config.args.clone().unwrap_or_default();

        args.extend(self.cargo_target_args());

        // Ensure the test_filter args are passed as well
        if let Some(ref filter) = self.config.test_filter {
            args.push(filter.clone());
        }
        args.extend(extra_args);

        // Some arguments are not compatible with the --no-capture arg
        if !args.contains(&"--test-threads".to_string()) {
            args.push("--no-capture".to_string());
        }

        if self.retries > 0 {
            args.push("--retries".to_string());
            args.push(self.retries.to_string());
        }

        self.runner.test(
            &env_vars,
            &self.config.runner.env,
            &args,
            self.coverage,
            if self.coverage {
                Some(self.environment.as_str())
            } else {
                None
            },
        )?;

        Ok(())
    }

    /// Cargo arguments selecting the features and test target to compile.
    fn cargo_target_args(&self) -> Vec<String> {
        // Integration tests always compile only the suite's own `test.yaml` features: each CI
        // job runs a single suite, so compiling `all-integration-tests` there only adds build
        // time. E2E tests on the shared runner still use `all-e2e-tests`.
        let shared_feature = (self.runner.is_shared_runner()
            && self.local_config.kind == ComposeTestKind::E2E)
            .then_some(self.local_config.feature_flag);
        let features = cargo_features(shared_feature, &self.config.features);
        let mut args = vec!["--features".to_string(), features];

        // If the test field is not present then use the --lib flag
        match self.config.test {
            Some(ref test_arg) => args.extend(["--test".to_string(), test_arg.clone()]),
            None => args.push("--lib".to_string()),
        }
        args
    }

    /// Compile the tests in the runner and package the resulting Vector binary as the image that
    /// the compose `vector` services run.
    ///
    /// Cargo builds the `vector` binary for integration test targets, using the same features
    /// and target directory as the test binary. The test phase then reuses these artifacts
    /// instead of compiling Vector a second time. The image is only re-committed when the
    /// runner image or the binary changed, so later environments of a run reuse it.
    fn build_e2e_vector_image(&self) -> Result<()> {
        // Only test targets build the `vector` binary. Without one, a stale binary left in the
        // shared target volume by another suite would end up in the image.
        if self.config.test.is_none() {
            bail!(
                "E2E test {} must set `test` in test.yaml: its compose services run the Vector \
                 binary built for that test target",
                self.test_name
            );
        }
        self.runner.ensure_network()?;
        self.runner.ensure_external_volumes()?;
        self.runner.ensure_running(&self.env_config)?;
        self.runner
            .build_tests(&self.cargo_target_args(), &e2e_snapshot_script())?;

        let container = self.runner.container_name();
        let output =
            docker_command(["exec", &container, "cat", E2E_VECTOR_DIGEST]).check_output()?;
        let Some(digest) = parse_sha256sum(&output) else {
            bail!("Unexpected sha256sum output: {output:?}");
        };
        let runner_image =
            docker_command(["inspect", "--format", "{{.Image}}", &container]).check_output()?;
        let runner_image = inspect_image(runner_image.trim())?;

        let image = e2e_vector_image_name(&container);
        // A missing or unreadable image is simply rebuilt.
        let previous = inspect_image(&image).ok();
        if previous
            .as_ref()
            .is_some_and(|previous| previous.is_committed_from(&runner_image, digest))
        {
            info!("Reusing {image}: Vector binary unchanged");
            return Ok(());
        }

        docker_command([
            "commit",
            "--change",
            &format!("LABEL {E2E_VECTOR_BINARY_LABEL}={digest}"),
            &container,
            &image,
        ])
        .wait(format!("Committing {image}"))?;

        // Each commit leaves the previous image dangling, and it holds a full debug binary.
        if let Some(previous) = previous
            && let Err(error) = docker_command(["image", "rm", &previous.id]).check_output()
        {
            debug!(
                "Could not remove previous {image} ({}): {error}",
                previous.id
            );
        }
        Ok(())
    }

    pub(crate) fn start(&self) -> Result<()> {
        let runner_was_missing = matches!(self.runner.state()?, RunnerState::Missing);
        let result = self.start_environment();
        // E2E start creates the runner before compose is up. If it fails, `stop` finds no active
        // environment and would leave the runner behind, attached to this test's network, for
        // the next test to reuse.
        if result.is_err()
            && runner_was_missing
            && let Err(error) = self.runner.remove()
        {
            warn!("Failed to remove runner after start failed: {error}");
        }
        result
    }

    fn start_environment(&self) -> Result<()> {
        // For end-to-end tests, we want to run vector as a service. Build it in the runner so
        // the compile is shared with the test binary. Coverage only instruments the test binary,
        // so the same uninstrumented service binary works for coverage runs.
        if self.local_config.kind == ComposeTestKind::E2E {
            self.build_e2e_vector_image()?;
        }

        self.config.check_required()?;
        if let Some(compose) = &self.compose {
            self.runner.ensure_network()?;
            self.runner.ensure_external_volumes()?;

            if self.is_running()? {
                bail!("environment is already up");
            }

            let project_name = self.project_name();
            compose.start(&self.env_config, &project_name)?;
        }
        Ok(())
    }

    pub(crate) fn stop(&self) -> Result<()> {
        if let Some(compose) = &self.compose {
            if !self.is_running()? {
                bail!("No environment for {} is up.", self.test_name);
            }

            let project_name = self.project_name();
            compose.stop(&self.env_config, &project_name)?;
        }

        self.runner.remove()?;

        Ok(())
    }

    fn logs(&self) -> Result<Option<ExitStatus>> {
        let Some(compose) = &self.compose else {
            return Ok(None);
        };

        compose
            .logs(&self.env_config, &self.project_name())
            .map(Some)
    }
}

#[derive(Debug)]
struct Compose {
    yaml_path: PathBuf,
    test_dir: PathBuf,
    env: Environment,
    #[cfg_attr(target_family = "windows", allow(dead_code))]
    config: ComposeConfig,
    network: String,
}

impl Compose {
    fn new(test_dir: PathBuf, env: Environment, network: String) -> Result<Option<Self>> {
        let yaml_path: PathBuf = [&test_dir, Path::new("compose.yaml")].iter().collect();

        match yaml_path.try_exists() {
            Err(error) => {
                Err(error).with_context(|| format!("Could not lookup {}", yaml_path.display()))
            }
            Ok(false) => Ok(None),
            Ok(true) => {
                // Parse config only for unix volume permission checking
                let config = ComposeConfig::parse(&yaml_path)?;

                Ok(Some(Self {
                    yaml_path,
                    test_dir,
                    env,
                    config,
                    network,
                }))
            }
        }
    }

    fn start(&self, environment: &Environment, project_name: &str) -> Result<()> {
        #[cfg(unix)]
        unix::prepare_compose_volumes(&self.config, &self.test_dir, environment)?;

        self.run(
            "Starting",
            &["up", "--detach"],
            Some(environment),
            project_name,
        )
    }

    fn stop(&self, environment: &Environment, project_name: &str) -> Result<()> {
        self.run(
            "Stopping",
            &["down", "--timeout", "0", "--volumes", "--remove-orphans"],
            Some(environment),
            project_name,
        )
    }

    fn run(
        &self,
        action: &str,
        args: &[&'static str],
        environment: Option<&Environment>,
        project_name: &str,
    ) -> Result<()> {
        let mut command = self.command(project_name);
        command.args(args);
        if let Some(environment) = environment {
            command.envs(extract_present(environment));
        }

        waiting!("{action} service environment");
        command.check_run()
    }

    fn logs(&self, environment: &Environment, project_name: &str) -> Result<ExitStatus> {
        // Preserve exit and OOM details before cleanup removes the containers.
        if let Err(error) = self.container_states(project_name) {
            warn!("Failed to collect container states: {error}");
        }

        let mut command = self.command(project_name);
        command.arg("logs");
        command.envs(extract_present(environment));
        command
            .status()
            .with_context(|| "Failed to collect compose logs")
    }

    fn container_states(&self, project_name: &str) -> Result<()> {
        let containers = docker_command([
            "ps",
            "--all",
            "--quiet",
            "--filter",
            &format!("label=com.docker.compose.project={project_name}"),
        ])
        .check_output()?;

        if !containers.trim().is_empty() {
            info!("Container states for compose project '{project_name}':");
            docker_command(["inspect", "--format", "{{.Name}} {{json .State}}"])
                .args(containers.split_whitespace())
                .check_run()?;
        }

        Ok(())
    }

    fn command(&self, project_name: &str) -> Command {
        let mut command = Command::new(CONTAINER_TOOL.clone());
        command.arg("compose");
        command.arg("--project-name");
        command.arg(project_name);
        command.arg("--file");
        command.arg(&self.yaml_path);
        command.current_dir(&self.test_dir);
        command.env("DOCKER_SOCKET", &*DOCKER_SOCKET);
        command.env(NETWORK_ENV_VAR, &self.network);

        // Some services require this in order to build Vector.
        command.env("RUST_VERSION", RustToolchainConfig::rust_version());
        command.envs(extract_present(&self.env));
        command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_e2e_vector_image_after_its_runner() {
        assert_eq!(
            e2e_vector_image_name("vector-test-runner-datadog-logs-1.96.1"),
            "vector-test-runner-datadog-logs-1.96.1-e2e-vector:latest"
        );
    }

    #[test]
    fn cargo_features_keep_suite_features_alongside_shared_feature() {
        let suite = [
            "e2e-tests-opentelemetry".to_string(),
            "typed-trace".to_string(),
        ];
        assert_eq!(
            cargo_features(Some("all-e2e-tests"), &suite),
            "all-e2e-tests,e2e-tests-opentelemetry,typed-trace,vendored"
        );
        assert_eq!(
            cargo_features(None, &suite),
            "e2e-tests-opentelemetry,typed-trace,vendored"
        );
        assert_eq!(
            cargo_features(Some("all-e2e-tests"), &["all-e2e-tests".to_string()]),
            "all-e2e-tests,vendored"
        );
    }

    #[test]
    fn snapshot_hashes_the_copied_binary() {
        // Hashing the copy, not the shared `debug/vector`, keeps the label matching the
        // committed binary even if another suite relinks the shared one afterwards.
        assert_eq!(
            e2e_snapshot_script(),
            "cp /home/target/debug/vector /usr/bin/vector && \
             sha256sum /usr/bin/vector > /tmp/vector.sha256"
        );
    }

    #[test]
    fn parses_sha256sum_output() {
        let digest = "b68fc0af0e9e62e5c2a2df3fc1bb5d0c3c8f2bd0e5d6c9a1f7a6e1b2c3d4e5f6";
        assert_eq!(
            parse_sha256sum(&format!("{digest}  /usr/bin/vector\n")),
            Some(digest)
        );
        assert_eq!(parse_sha256sum(""), None);
        assert_eq!(
            parse_sha256sum("sha256sum: /usr/bin/vector: No such file or directory"),
            None
        );
    }

    const RUNNER_INSPECT: &str = r#"[{
        "Id": "sha256:runner",
        "Config": {
            "Env": ["PATH=/usr/local/cargo/bin:/usr/bin"], "Entrypoint": null, "User": "",
            "Cmd": ["bash"], "WorkingDir": "/vector",
            "Labels": {"vector-test-runner": "true"}
        },
        "RootFS": {"Type": "layers", "Layers": ["sha256:a", "sha256:b"]}
    }]"#;

    // vdev creates the runner container with its own command and working directory, then
    // commits it with the binary label and one extra layer.
    const COMMITTED_INSPECT: &str = r#"[{
        "Id": "sha256:e2e",
        "Config": {
            "Env": ["PATH=/usr/local/cargo/bin:/usr/bin"], "Entrypoint": null, "User": "",
            "Cmd": ["/bin/sleep", "infinity"], "WorkingDir": "/home/vector",
            "Labels": {"vector-test-runner": "true", "vector.e2e.binary-sha256": "abc"}
        },
        "RootFS": {"Type": "layers", "Layers": ["sha256:a", "sha256:b", "sha256:c"]}
    }]"#;

    #[test]
    fn parses_docker_image_inspect_output() {
        let runner = ImageInspect::parse(RUNNER_INSPECT).unwrap();
        let committed = ImageInspect::parse(COMMITTED_INSPECT).unwrap();
        assert!(committed.is_committed_from(&runner, "abc"));

        // Docker prints `null` for images without labels.
        let unlabeled = ImageInspect::parse(
            r#"[{"Id": "sha256:x", "Config": {"Labels": null}, "RootFS": {"Layers": []}}]"#,
        )
        .unwrap();
        assert_eq!(unlabeled.label(E2E_VECTOR_BINARY_LABEL), None);

        assert!(ImageInspect::parse("[]").is_err());
        assert!(ImageInspect::parse("Error: No such image").is_err());
    }

    fn changed(image: &ImageInspect, change: impl FnOnce(&mut ImageInspect)) -> ImageInspect {
        let mut image = image.clone();
        change(&mut image);
        image
    }

    #[test]
    fn reuses_image_only_while_runner_and_binary_are_unchanged() {
        let runner = ImageInspect::parse(RUNNER_INSPECT).unwrap();
        let committed = ImageInspect::parse(COMMITTED_INSPECT).unwrap();

        // A cached rebuild of the runner can get a new ID without changing anything else.
        let rebuilt = changed(&runner, |image| {
            image.id = "sha256:rebuilt".to_string();
        });
        assert!(committed.is_committed_from(&rebuilt, "abc"));

        assert!(!committed.is_committed_from(&runner, "def"));
        let not_committed = [
            changed(&runner, |image| {
                image.root_fs.layers[1] = "sha256:x".to_string();
            }),
            // Config-only Dockerfile changes keep the same layers.
            changed(&runner, |image| {
                image.config.env = Some(vec!["FOO=bar".to_string()]);
            }),
            changed(&runner, |image| {
                image.config.entrypoint = Some(vec!["/bin/sh".to_string()]);
            }),
            changed(&runner, |image| {
                image.config.user = Some("vector".to_string());
            }),
        ];
        for runner in not_committed {
            assert!(!committed.is_committed_from(&runner, "abc"), "{runner:?}");
        }

        let unlabeled = changed(&committed, |image| {
            image.config.labels = None;
        });
        assert!(!unlabeled.is_committed_from(&runner, "abc"));
        // The runner image itself is never mistaken for a committed image.
        let labeled_runner = changed(&runner, |image| {
            image.config.labels = committed.config.labels.clone();
        });
        assert!(!labeled_runner.is_committed_from(&runner, "abc"));
    }
}

#[cfg(unix)]
mod unix {
    use std::{
        fs::{self, Metadata, Permissions},
        os::unix::fs::PermissionsExt as _,
        path::{Path, PathBuf},
    };

    use anyhow::{Context, Result};

    use super::super::config::ComposeConfig;
    use crate::{
        testing::config::VolumeMount,
        utils::environment::{Environment, resolve_placeholders},
    };

    /// Unix permissions mask to allow everybody to read a file
    const ALL_READ: u32 = 0o444;
    /// Unix permissions mask to allow everybody to read a directory
    const ALL_READ_DIR: u32 = 0o555;

    /// Fix up potential issues before starting a compose container
    pub fn prepare_compose_volumes(
        config: &ComposeConfig,
        test_dir: &Path,
        environment: &Environment,
    ) -> Result<()> {
        for service in config.services.values() {
            if let Some(volumes) = &service.volumes {
                for volume in volumes {
                    let source = match volume {
                        VolumeMount::Short(s) => {
                            s.split_once(':').map(|(s, _)| s).ok_or_else(|| {
                                anyhow::anyhow!("Invalid short volume mount format: {s}")
                            })?
                        }
                        VolumeMount::Long { source, .. } => source,
                    };
                    let source = resolve_placeholders(source, environment);
                    if !config.volumes.contains_key(&source)
                        && !source.starts_with('/')
                        && !source.starts_with('$')
                    {
                        let path: PathBuf = [test_dir, Path::new(&source)].iter().collect();
                        add_read_permission(&path)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Recursively add read permissions to the
    fn add_read_permission(path: &Path) -> Result<()> {
        let metadata = path
            .metadata()
            .with_context(|| format!("Could not get permissions on {}", path.display()))?;

        if metadata.is_file() {
            add_permission(path, &metadata, ALL_READ)
        } else {
            if metadata.is_dir() {
                add_permission(path, &metadata, ALL_READ_DIR)?;
                for entry in fs::read_dir(path)
                    .with_context(|| format!("Could not read directory {}", path.display()))?
                {
                    let entry = entry.with_context(|| {
                        format!("Could not read directory entry in {}", path.display())
                    })?;
                    add_read_permission(&entry.path())?;
                }
            }
            Ok(())
        }
    }

    fn add_permission(path: &Path, metadata: &Metadata, bits: u32) -> Result<()> {
        let perms = metadata.permissions();
        let new_perms = Permissions::from_mode(perms.mode() | bits);
        if new_perms != perms {
            fs::set_permissions(path, new_perms)
                .with_context(|| format!("Could not set permissions on {}", path.display()))?;
        }
        Ok(())
    }
}

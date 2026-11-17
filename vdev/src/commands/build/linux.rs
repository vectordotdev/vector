use std::{env, fs, path::Path, process::Command};

use anyhow::{Context as _, Result, ensure};
use clap::{Args, ValueEnum};
use regex::Regex;

use crate::app::{self, CommandExt as _};

const LINUX_TARGETS: [&str; 8] = [
    "x86_64-unknown-linux-gnu",
    "aarch64-unknown-linux-gnu",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
    "arm-unknown-linux-gnueabi",
    "armv7-unknown-linux-gnueabihf",
    "arm-unknown-linux-musleabi",
    "armv7-unknown-linux-musleabihf",
];
const BOOKWORM: &str =
    "debian:bookworm-slim@sha256:734a72b3d6142a71993b4bc580ca01d8ca687f77a2c422d96a961d3376b179bc";
const TRIXIE: &str =
    "debian:trixie-slim@sha256:a72b4433de1cf7d6bae0c4cee74d0547f58601a643b3375f070c51600d63b6b2";
const SMOKE_CONFIG: &str = "/work/tests/data/native-release-smoke.yaml";
const PROBE_MANIFEST: &str = "tests/data/native-release-toolchain-probe/Cargo.toml";
const EXPECTED_MESSAGE: &str = "\"message\":\"NATIVE PACKAGING WORKS\"";

/// Build Linux targets with native container tools, without cross-rs or QEMU.
#[derive(Args, Debug)]
pub struct Cli {
    /// Original Rust release triple. ARM32 requires an ARM64 host.
    target: String,

    #[arg(long, value_enum, default_value = "build")]
    command: Operation,

    #[arg(long, value_enum, default_value = "release")]
    profile: Profile,

    #[arg(long, value_enum, env = "CONTAINER_TOOL", default_value = "docker")]
    container_tool: ContainerTool,

    /// Build the toolchain image without compiling Vector.
    #[arg(long, conflicts_with_all = ["probe_only", "verify"])]
    image_only: bool,

    /// Compile and execute only the ARM32 thread/atomic/clock/FP probe.
    #[arg(long, conflicts_with = "verify")]
    probe_only: bool,

    /// Verify the release ABI, static linking, and real stdin/remap/console pipeline.
    #[arg(long)]
    verify: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Operation {
    Build,
    Check,
    Test,
    Clippy,
}

impl Operation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Check => "check",
            Self::Test => "test",
            Self::Clippy => "clippy",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
enum Profile {
    Debug,
    Release,
}

impl Profile {
    const fn directory(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum ContainerTool {
    Docker,
    Podman,
}

impl ContainerTool {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Docker => "docker",
            Self::Podman => "podman",
        }
    }
}

#[derive(Debug)]
struct Target {
    triple: &'static str,
    host_arch: &'static str,
    rust_host: &'static str,
    dockerfile: &'static str,
}

impl Target {
    fn parse(target: &str) -> Result<Self> {
        let triple = LINUX_TARGETS
            .into_iter()
            .find(|triple| *triple == target)
            .with_context(|| format!("Unsupported Linux release target: {target}"))?;
        let arm32 = triple.starts_with("arm-") || triple.starts_with("armv7-");
        let host_arch = if triple.starts_with("x86_64-") {
            "x86_64"
        } else {
            "aarch64"
        };
        let rust_host = if arm32 {
            "aarch64-unknown-linux-gnu"
        } else {
            triple
        };
        let dockerfile = if arm32 {
            "scripts/native/Dockerfile.arm-gnu"
        } else if triple.contains("musl") {
            "scripts/native/Dockerfile.alpine"
        } else {
            "scripts/native/Dockerfile.gnu"
        };
        Ok(Self {
            triple,
            host_arch,
            rust_host,
            dockerfile,
        })
    }

    fn platform(&self) -> &'static str {
        if self.host_arch == "x86_64" {
            "linux/amd64"
        } else {
            "linux/arm64"
        }
    }

    fn arm32(&self) -> bool {
        self.triple.starts_with("arm-") || self.triple.starts_with("armv7-")
    }

    fn armv6(&self) -> bool {
        self.triple.starts_with("arm-")
    }

    fn musl(&self) -> bool {
        self.triple.contains("musl")
    }

    fn zig(&self, operation: Operation) -> bool {
        self.arm32() && !self.musl() && operation == Operation::Build
    }

    fn image(&self) -> String {
        format!("vector-native-builder:{}", self.triple)
    }

    fn validate_host(&self, arch: &str) -> Result<()> {
        ensure!(
            arch == self.host_arch,
            "{} requires a {} host; refusing emulated build tools on {arch}",
            self.triple,
            self.host_arch,
        );
        Ok(())
    }
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        let target = Target::parse(&self.target)?;
        target.validate_host(env::consts::ARCH)?;
        reject_qemu(Path::new("/proc/sys/fs/binfmt_misc"))?;
        let template = match self.container_tool {
            ContainerTool::Docker => "{{.Architecture}}",
            ContainerTool::Podman => "{{.Host.Arch}}",
        };
        let architecture = self
            .engine()
            .args(["info", "--format", template])
            .check_output()?;
        let architecture = match architecture.trim() {
            "amd64" | "x86_64" => "x86_64",
            "arm64" | "aarch64" => "aarch64",
            architecture => architecture,
        };
        target.validate_host(architecture)?;
        if self.verify || self.probe_only {
            ensure!(
                self.command == Operation::Build,
                "Verification requires --command build"
            );
            ensure!(
                self.profile == Profile::Release,
                "Verification requires --profile release"
            );
            validate_release_overrides()?;
        }
        if self.probe_only {
            ensure!(
                target.arm32(),
                "The toolchain probe requires an ARM32 target"
            );
        }
        if !self.image_only
            && target.armv6()
            && (self.verify || self.probe_only || self.command == Operation::Test)
        {
            let mode = fs::read_to_string("/proc/sys/abi/cp15_barrier").context(
                "ARMv6 execution requires an ARM64 Linux kernel with CP15 barrier support",
            )?;
            ensure!(
                mode.trim() == "2",
                "Enable native ARMv6 barriers before runtime checks: sudo sysctl -w abi.cp15_barrier=2",
            );
        }
        self.build_image(&target)?;
        if self.image_only {
            return Ok(());
        }
        let result = self.build_target(&target);
        let ownership = self.restore_ownership(&target);
        result.and(ownership)
    }

    fn build_target(&self, target: &Target) -> Result<()> {
        let version = self
            .run(target)
            .args(["timeout", "-k", "10", "90", "rustc", "-vV"])
            .check_output()?;
        let host = format!("host: {}", target.rust_host);
        ensure!(
            version.lines().any(|line| line == host),
            "Unexpected Rust tool host: {version}"
        );
        info!("{version}");
        if target.arm32() && (self.verify || self.probe_only) {
            let mut command = self.run_with_target_dir(target);
            command.args(self.cargo_args(target, true));
            command.args(["--manifest-path", PROBE_MANIFEST]);
            command.check_run()?;
            let binary = format!(
                "/work/target/native-release-probe/{}/release/native-release-toolchain-probe",
                target.triple
            );
            let versions = self
                .run(target)
                .args(["readelf", "--version-info", &binary])
                .check_output()?;
            if !target.musl() {
                check_glibc(&versions, (2, 17, 0))?;
            }
            self.run(target)
                .args(["timeout", "-k", "10", "90", &binary])
                .check_run()?;
        }
        if self.probe_only {
            return Ok(());
        }
        self.cargo(target).check_run()?;
        if self.command == Operation::Build {
            // Do not reuse host build scripts between AlmaLinux, Debian, and
            // Alpine. Publish only the executable at the existing Makefile path.
            let directory = format!(
                "/work/target/{}/{}",
                target.triple,
                self.profile.directory()
            );
            let built = format!(
                "/work/target/native-linux/{}/{}/{}/vector",
                target.triple,
                target.triple,
                self.profile.directory(),
            );
            self.run(target)
                .args(["mkdir", "-p", &directory])
                .check_run()?;
            self.run(target)
                .args(["cp", "-p", &built, &format!("{directory}/vector")])
                .check_run()?;
        }
        if self.verify {
            self.verify_binary(target)?;
        }
        Ok(())
    }

    fn restore_ownership(&self, target: &Target) -> Result<()> {
        // Restore only build outputs, including on failure. Read the checkout's
        // owner inside the container to respect rootless UID/GID mappings too.
        if Path::new(app::path()).join("target").exists() {
            self.run(target)
                .args([
                    "sh",
                    "-ec",
                    r#"chown -R "$(stat -c '%u:%g' /work)" /work/target"#,
                ])
                .check_run()?;
        }
        Ok(())
    }

    fn engine(&self) -> Command {
        let mut command = Command::new(self.container_tool.as_str());
        command.in_repo();
        command
    }

    fn build_image(&self, target: &Target) -> Result<()> {
        let manifest = fs::read_to_string(Path::new(app::path()).join("rust-toolchain.toml"))?;
        let manifest: toml::Value = toml::from_str(&manifest)?;
        let version = manifest
            .get("toolchain")
            .and_then(|toolchain| toolchain.get("channel"))
            .and_then(toml::Value::as_str)
            .context("rust-toolchain.toml must set toolchain.channel")?;
        let base = if target.triple == "arm-unknown-linux-gnueabi" {
            TRIXIE
        } else {
            BOOKWORM
        };
        self.engine()
            .args([
                "build",
                "--pull",
                "--platform",
                target.platform(),
                "-f",
                target.dockerfile,
            ])
            .args(["--build-arg", &format!("BASE={base}")])
            .args(["--build-arg", &format!("RUST_VERSION={version}")])
            .args([
                "--build-arg",
                &format!("INSTALL_CLIPPY={}", self.command == Operation::Clippy),
            ])
            .args(["--build-arg", &format!("RUST_HOST={}", target.rust_host)])
            .args(["--build-arg", &format!("RUST_TARGET={}", target.triple)])
            .args(["-t", &target.image(), "."])
            .check_run()?;
        if target.arm32() && target.musl() {
            self.engine()
                .args(["tag", &target.image(), "vector-native-arm-gnu"])
                .check_run()?;
            let mut command = self.engine();
            command
                .args([
                    "build",
                    "--platform",
                    target.platform(),
                    "-f",
                    "scripts/native/Dockerfile.arm-musl",
                ])
                .args(["--build-arg", &format!("RUST_VERSION={version}")])
                .args(["--build-arg", &format!("RUST_TARGET={}", target.triple)]);
            if target.triple == "armv7-unknown-linux-musleabihf" {
                for argument in [
                    "MUSL_TARGET=arm-linux-musleabihf",
                    "GCC_ARCH=armv7-a",
                    "GCC_FLOAT=hard",
                    "GCC_FPU=vfpv3-d16",
                ] {
                    command.args(["--build-arg", argument]);
                }
            }
            command.args(["-t", &target.image(), "."]).check_run()?;
        }
        Ok(())
    }

    fn run_options(&self, target: &Target, probe: bool) -> Command {
        let mut command = self.engine();
        command
            .args([
                "run",
                "--rm",
                "--platform",
                target.platform(),
                "--workdir",
                "/work",
            ])
            .args(["--volume", &format!("{}:/work", app::path())]);
        for variable in [
            "BUILD_DIR",
            "RUST_BACKTRACE",
            "RUST_LOG",
            "VECTOR_VERSION",
            "VECTOR_BUILD_DESC",
            "CHANNEL",
            "JEMALLOC_SYS_WITH_LG_PAGE",
            "JEMALLOC_SYS_WITH_LG_HUGEPAGE",
            "CARGO_BUILD_JOBS",
        ] {
            if env::var_os(variable).is_some() {
                command.args(["--env", variable]);
            }
        }
        command.args(["--env", "CARGO_INCREMENTAL=0"]);
        let target_dir = if probe {
            "/work/target/native-release-probe".to_owned()
        } else {
            format!("/work/target/native-linux/{}", target.triple)
        };
        command.args(["--env", &format!("CARGO_TARGET_DIR={target_dir}")]);
        let flags = env::var("CFLAGS").unwrap_or_else(|_| "-g0 -O3".to_owned());
        command.args(["--env", &format!("CFLAGS={flags}")]);
        if target.musl() {
            let flags = env::var("RUSTFLAGS").unwrap_or_else(|_| "-D warnings".to_owned());
            command.args([
                "--env",
                &format!("RUSTFLAGS={flags} -C target-feature=+crt-static"),
            ]);
        } else if env::var_os("RUSTFLAGS").is_some() {
            command.args(["--env", "RUSTFLAGS"]);
        }
        command
    }

    fn run(&self, target: &Target) -> Command {
        let mut command = self.run_options(target, false);
        command.arg(target.image());
        command
    }

    fn run_with_target_dir(&self, target: &Target) -> Command {
        let mut command = self.run_options(target, true);
        command.arg(target.image());
        command
    }

    fn cargo_args(&self, target: &Target, probe: bool) -> Vec<String> {
        let mut args = Vec::new();
        let triple = if target.zig(self.command) {
            let key = target.triple.replace('-', "_");
            args.push("env".to_owned());
            for variable in [
                format!("CC_{key}"),
                format!("CXX_{key}"),
                format!("AR_{key}"),
                format!("CARGO_TARGET_{}_LINKER", key.to_uppercase()),
            ] {
                args.extend(["-u".to_owned(), variable]);
            }
            args.extend([
                format!("PKG_CONFIG_LIBDIR_{key}=/opt/target-pkgconfig"),
                "LIBZ_SYS_STATIC=1".to_owned(),
            ]);
            args.extend(["cargo".to_owned(), "zigbuild".to_owned()]);
            format!("{}.2.17", target.triple)
        } else {
            args.extend(["cargo".to_owned(), self.command.as_str().to_owned()]);
            target.triple.to_owned()
        };
        args.extend(["--locked".to_owned(), "--target".to_owned(), triple]);
        if self.profile == Profile::Release {
            args.push("--release".to_owned());
        }
        if !probe {
            args.extend([
                "--no-default-features".to_owned(),
                "--features".to_owned(),
                format!("target-{}", target.triple),
            ]);
        }
        args
    }

    fn cargo(&self, target: &Target) -> Command {
        let mut command = self.run(target);
        command.args(self.cargo_args(target, false));
        command
    }

    fn verify_binary(&self, target: &Target) -> Result<()> {
        let binary = format!(
            "/work/target/{}/{}/vector",
            target.triple,
            self.profile.directory()
        );
        self.run(target)
            .args(["readelf", "-h", &binary])
            .check_run()?;
        self.run(target)
            .args(["readelf", "-A", &binary])
            .check_run()?;
        let versions = self
            .run(target)
            .args(["readelf", "--version-info", &binary])
            .check_output()?;
        if target.musl() {
            let headers = self
                .run(target)
                .args(["readelf", "-l", &binary])
                .check_output()?;
            let dynamic = self
                .run(target)
                .args(["readelf", "-d", &binary])
                .check_output()?;
            ensure!(
                !headers.contains("INTERP"),
                "musl artifact must not have a dynamic interpreter"
            );
            ensure!(
                !dynamic.contains("(NEEDED)"),
                "musl artifact must not have dynamic dependencies"
            );
        } else {
            check_glibc(
                &versions,
                if target.arm32() {
                    (2, 17, 0)
                } else {
                    (2, 28, 0)
                },
            )?;
        }
        if target.triple == "x86_64-unknown-linux-gnu" {
            self.run(target)
                .args(["grep", "-q", "gssapiv2_client_plug_init", &binary])
                .check_run()
                .context("GNU x86-64 artifact must retain its Cyrus SASL GSSAPI plugin")?;
        }
        self.run(target)
            .args(["timeout", "-k", "10", "90", &binary, "--version"])
            .check_run()?;
        let output = self
            .run(target)
            .args([
                "bash",
                "-euo",
                "pipefail",
                "-c",
                "printf 'native packaging works\\n' | timeout -k 10 90 \"$1\" --config \"$2\"",
                "--",
                &binary,
                SMOKE_CONFIG,
            ])
            .check_output()?;
        ensure!(
            output.contains(EXPECTED_MESSAGE),
            "Release pipeline did not produce the expected message: {output}"
        );
        info!("Release pipeline passed for {}", target.triple);
        Ok(())
    }
}

fn check_glibc(output: &str, ceiling: (u64, u64, u64)) -> Result<()> {
    let pattern = Regex::new(r"GLIBC_([0-9]+)\.([0-9]+)(?:\.([0-9]+))?")?;
    let mut maximum: Option<(u64, u64, u64)> = None;
    for capture in pattern.captures_iter(output) {
        let version = (
            capture[1].parse()?,
            capture[2].parse()?,
            capture.get(3).map_or(Ok(0), |part| part.as_str().parse())?,
        );
        maximum = Some(maximum.map_or(version, |previous| previous.max(version)));
    }
    let maximum = maximum.context("GNU artifact has no GLIBC symbol requirements")?;
    ensure!(
        maximum <= ceiling,
        "GNU artifact requires GLIBC_{maximum:?}, above {ceiling:?}"
    );
    info!(
        "Maximum GNU symbol requirement: GLIBC_{}.{}.{}",
        maximum.0, maximum.1, maximum.2
    );
    Ok(())
}

fn validate_release_overrides() -> Result<()> {
    for (variable, expected) in [
        ("CARGO_PROFILE_RELEASE_LTO", "fat"),
        ("CARGO_PROFILE_RELEASE_CODEGEN_UNITS", "1"),
        ("CARGO_PROFILE_RELEASE_DEBUG", "false"),
    ] {
        if let Ok(value) = env::var(variable) {
            ensure!(
                value == expected,
                "Release verification requires {variable}={expected}, not {value}"
            );
        }
    }
    Ok(())
}

fn reject_qemu(directory: &Path) -> Result<()> {
    if !directory.exists() {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        if let Ok(contents) = fs::read_to_string(entry.path()) {
            ensure!(
                !contents.to_lowercase().contains("qemu"),
                "Native builds must not use a QEMU binfmt registration"
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::Parser as _;

    use super::{Cli, ContainerTool, Operation, Profile, Target, check_glibc, reject_qemu};

    fn cli(target: &str) -> Cli {
        Cli {
            target: target.to_owned(),
            command: Operation::Build,
            profile: Profile::Release,
            container_tool: ContainerTool::Docker,
            image_only: false,
            probe_only: false,
            verify: true,
        }
    }

    #[test]
    fn all_release_targets_use_matching_native_host_tools() {
        for triple in [
            "x86_64-unknown-linux-gnu",
            "aarch64-unknown-linux-gnu",
            "x86_64-unknown-linux-musl",
            "aarch64-unknown-linux-musl",
            "arm-unknown-linux-gnueabi",
            "armv7-unknown-linux-gnueabihf",
            "arm-unknown-linux-musleabi",
            "armv7-unknown-linux-musleabihf",
        ] {
            let target = Target::parse(triple).unwrap();
            assert!(target.validate_host(target.host_arch).is_ok());
            assert!(target.validate_host("unsupported").is_err());
            let args = cli(triple).cargo_args(&target, false);
            assert!(args.contains(&"--release".to_owned()));
            assert!(args.contains(&"--locked".to_owned()));
            assert!(args.contains(&format!("target-{triple}")));
            assert!(
                !args
                    .iter()
                    .any(|arg| arg == "cross" || arg.contains("qemu"))
            );
            if target.arm32() {
                assert_eq!(target.rust_host, "aarch64-unknown-linux-gnu");
            }
        }
        assert!(Target::parse("unsupported").is_err());
    }

    #[test]
    fn gnu_arm_builds_use_the_compatible_zig_sysroot() {
        let target = Target::parse("arm-unknown-linux-gnueabi").unwrap();
        let args = cli(target.triple).cargo_args(&target, false);
        assert!(args.contains(&"zigbuild".to_owned()));
        assert!(args.contains(&"arm-unknown-linux-gnueabi.2.17".to_owned()));
        assert!(args.contains(&"LIBZ_SYS_STATIC=1".to_owned()));
        assert!(args.contains(
            &"PKG_CONFIG_LIBDIR_arm_unknown_linux_gnueabi=/opt/target-pkgconfig".to_owned()
        ));
    }

    #[test]
    fn standalone_probe_does_not_receive_workspace_features() {
        let target = Target::parse("armv7-unknown-linux-musleabihf").unwrap();
        let args = cli(target.triple).cargo_args(&target, true);
        assert!(!args.contains(&"--features".to_owned()));
        assert!(!args.contains(&format!("target-{}", target.triple)));
    }

    #[test]
    fn test_commands_use_the_target_runtime_toolchain() {
        let target = Target::parse("arm-unknown-linux-gnueabi").unwrap();
        let mut command = cli(target.triple);
        command.command = Operation::Test;
        let args = command.cargo_args(&target, false);
        assert_eq!(&args[..2], &["cargo", "test"]);
        assert!(args.contains(&target.triple.to_owned()));
    }

    #[test]
    fn gnu_symbol_ceiling_is_numeric_and_fails_closed() {
        assert!(check_glibc("GLIBC_2.4 GLIBC_2.9 GLIBC_2.17", (2, 17, 0)).is_ok());
        assert!(check_glibc("GLIBC_2.34", (2, 17, 0)).is_err());
        assert!(check_glibc("GLIBC_2.17.1", (2, 17, 0)).is_err());
        assert!(check_glibc("no version information", (2, 17, 0)).is_err());
    }

    #[test]
    fn command_line_rejects_conflicting_build_modes() {
        #[derive(clap::Parser)]
        struct Arguments {
            #[command(flatten)]
            linux: Cli,
        }

        let arguments =
            Arguments::try_parse_from(["linux", "arm-unknown-linux-gnueabi", "--probe-only"])
                .unwrap();
        assert!(arguments.linux.probe_only);
        for mode in ["--verify", "--image-only"] {
            assert!(
                Arguments::try_parse_from([
                    "linux",
                    "arm-unknown-linux-gnueabi",
                    "--probe-only",
                    mode,
                ])
                .is_err()
            );
        }
    }

    #[test]
    fn debug_builds_do_not_select_the_release_profile() {
        let target = Target::parse("x86_64-unknown-linux-gnu").unwrap();
        let mut command = cli(target.triple);
        command.profile = Profile::Debug;
        assert!(
            !command
                .cargo_args(&target, false)
                .contains(&"--release".to_owned())
        );
    }

    #[test]
    fn qemu_registration_is_rejected_without_running_it() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("status"), "enabled").unwrap();
        assert!(reject_qemu(directory.path()).is_ok());
        std::fs::write(
            directory.path().join("qemu-arm"),
            "interpreter /usr/bin/qemu-arm-static",
        )
        .unwrap();
        assert!(reject_qemu(directory.path()).is_err());
    }
}

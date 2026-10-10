use std::{ffi::OsString, path::Path};

use crate::{cli::handle_config_errors, config};

pub(super) fn create_service_arguments(
    config_paths: &[config::ConfigPath],
    data_dir: Option<&Path>,
) -> Option<Vec<OsString>> {
    // The service may start in a different working directory from the installer.
    // Resolve the override once for both configuration loading and persisted arguments,
    // without requiring the directory to exist or resolving filesystem symlinks.
    let data_dir = match data_dir
        .map(|path| {
            if path.is_absolute() {
                Ok(path.to_path_buf())
            } else {
                std::path::absolute(path)
            }
        })
        .transpose()
    {
        Ok(path) => path,
        Err(error) => {
            error!(message = "Failed to resolve service data directory.", %error);
            return None;
        }
    };
    let data_dir = data_dir.as_deref();
    let config_paths = config::process_paths(config_paths)?;
    match config::loading::load_from_paths_with_data_dir(&config_paths, data_dir) {
        Ok(_) => {
            let mut args: Vec<OsString> = config_paths
                .iter()
                .flat_map(|config_path| match config_path {
                    config::ConfigPath::File(path, format) => {
                        let key = match format {
                            None => "--config",
                            Some(config::Format::Toml) => "--config-toml",
                            Some(config::Format::Json) => "--config-json",
                            Some(config::Format::Yaml) => "--config-yaml",
                        };
                        vec![OsString::from(key), path.as_os_str().into()]
                    }
                    config::ConfigPath::Dir(path) => {
                        vec![OsString::from("--config-dir"), path.as_os_str().into()]
                    }
                })
                .collect();
            if config::env_var_interpolation_enabled() {
                args.push(OsString::from("--dangerously-allow-env-var-interpolation"));
            }
            if let Some(data_dir) = data_dir {
                // Keep option and path as separate arguments. The service library owns
                // Windows command-line quoting, including spaces and trailing backslashes.
                args.push(OsString::from("--data-dir"));
                args.push(data_dir.as_os_str().to_owned());
            }
            Some(args)
        }
        Err(errs) => {
            handle_config_errors(errs);
            None
        }
    }
}

#[cfg(all(test, feature = "sources-demo_logs", feature = "sinks-blackhole"))]
mod tests {
    #[cfg(unix)]
    use std::os::unix::ffi::OsStringExt;
    #[cfg(windows)]
    use std::os::windows::ffi::OsStringExt;

    use clap::Parser;

    use super::*;

    fn write_config(path: &Path, data_dir: &Path, include_components: bool) {
        let data_dir = serde_json::to_string(data_dir).unwrap();
        let mut text = format!("data_dir: {data_dir}\n");
        if include_components {
            text.push_str("sources:\n  input:\n    type: demo_logs\n    format: shuffle\n    lines: [\"log\"]\nsinks:\n  output:\n    type: blackhole\n    inputs: [input]\n");
        }
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn service_arguments_leave_unset_data_dir_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector config.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        let args = create_service_arguments(&[config::ConfigPath::File(path.clone(), None)], None)
            .unwrap();
        let mut expected = vec![OsString::from("--config"), path.into_os_string()];
        if config::env_var_interpolation_enabled() {
            expected.push(OsString::from("--dangerously-allow-env-var-interpolation"));
        }
        assert_eq!(args, expected);
    }

    #[test]
    fn service_data_dir_keeps_spaces_and_backslashes_in_one_argument() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector config.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        let mut data_dir = directory.path().join("state with spaces");
        std::fs::create_dir(&data_dir).unwrap();
        data_dir.as_mut_os_string().push("\\");
        let args =
            create_service_arguments(&[config::ConfigPath::File(path, None)], Some(&data_dir))
                .unwrap();
        assert_eq!(args[args.len() - 2], "--data-dir");
        assert_eq!(args.last().unwrap().as_os_str(), data_dir.as_os_str());
        let parsed = crate::cli::RootOpts::try_parse_from(
            std::iter::once(OsString::from("vector")).chain(args),
        )
        .unwrap();
        assert_eq!(parsed.data_dir, Some(data_dir));
    }

    #[test]
    fn service_data_dir_override_precedes_split_config_merge() {
        let directory = tempfile::tempdir().unwrap();
        let first = directory.path().join("first.yaml");
        let second = directory.path().join("second.yaml");
        write_config(&first, &directory.path().join("first-state"), true);
        write_config(&second, &directory.path().join("second-state"), false);
        let paths = [
            config::ConfigPath::File(first, None),
            config::ConfigPath::File(second, None),
        ];
        assert!(create_service_arguments(&paths, None).is_none());
        let data_dir = directory.path().join("override-state");
        let args = create_service_arguments(&paths, Some(&data_dir)).unwrap();
        assert_eq!(args.last().unwrap().as_os_str(), data_dir.as_os_str());
    }

    fn run_in_installer_directory(test_name: &str) -> bool {
        const CHILD: &str = "VECTOR_TEST_SERVICE_DATA_DIR_CHILD";
        if std::env::var(CHILD).as_deref() == Ok(test_name) {
            return false;
        }
        let directory = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .env(CHILD, test_name)
            .env_remove("VECTOR_DATA_DIR")
            .env_remove("VECTOR_TEST_SERVICE_DATA_DIR_ARGUMENT")
            .current_dir(directory.path())
            .args(["--exact", test_name, "--nocapture"])
            .output()
            .unwrap();
        assert_subprocess_completed(&output);
        true
    }

    fn assert_subprocess_completed(output: &std::process::Output) {
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("service data-dir assertions completed")
        );
    }

    #[test]
    fn service_data_dir_resolves_relative_path_before_launch() {
        const TEST: &str =
            "service_arguments::tests::service_data_dir_resolves_relative_path_before_launch";
        const ARGUMENT: &str = "VECTOR_TEST_SERVICE_DATA_DIR_ARGUMENT";
        const EXPECTED: &str = "VECTOR_TEST_SERVICE_DATA_DIR_EXPECTED";
        const CONFIG: &str = "VECTOR_TEST_SERVICE_DATA_DIR_CONFIG";
        if run_in_installer_directory(TEST) {
            return;
        }

        if let Some(argument) = std::env::var_os(ARGUMENT) {
            // Replay the persisted argument from a different service working directory.
            let expected = std::path::PathBuf::from(std::env::var_os(EXPECTED).unwrap());
            assert_ne!(std::env::current_dir().unwrap(), expected.parent().unwrap());
            let opts = crate::cli::RootOpts::try_parse_from([
                OsString::from("vector"),
                OsString::from("--data-dir"),
                argument,
            ])
            .unwrap();
            assert_eq!(opts.data_dir.as_ref(), Some(&expected));
            let paths = [config::ConfigPath::File(
                std::env::var_os(CONFIG).unwrap().into(),
                None,
            )];
            let loaded =
                config::loading::load_from_paths_with_data_dir(&paths, opts.data_dir.as_deref())
                    .unwrap();
            assert_eq!(loaded.global.data_dir, Some(expected));
        } else {
            let installer_directory = std::env::current_dir().unwrap();
            let config = installer_directory.join("vector config.yaml");
            write_config(&config, &installer_directory.join("configured-state"), true);
            let relative = Path::new(".").join("state with spaces");
            let expected = installer_directory.join("state with spaces");
            assert!(!expected.exists());
            let args = create_service_arguments(
                &[config::ConfigPath::File(config.clone(), None)],
                Some(&relative),
            )
            .unwrap();
            let argument = args.last().unwrap();
            assert!(Path::new(argument).is_absolute());
            assert_eq!(Path::new(argument), expected);
            assert_eq!(args[args.len() - 2], "--data-dir");
            assert!(!expected.exists());

            let service_directory = installer_directory.join("service-working-directory");
            std::fs::create_dir(&service_directory).unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .env(ARGUMENT, argument)
                .env(EXPECTED, expected)
                .env(CONFIG, config)
                .current_dir(service_directory)
                .args(["--exact", TEST, "--nocapture"])
                .output()
                .unwrap();
            assert_subprocess_completed(&output);
        }
        #[allow(
            clippy::print_stdout,
            reason = "The parent verifies that each isolated service-path test ran."
        )]
        {
            println!("service data-dir assertions completed");
        }
    }

    #[test]
    fn service_data_dir_preserves_non_unicode_path() {
        #[cfg(unix)]
        let relative = OsString::from_vec(b"state-\xff".to_vec());
        #[cfg(windows)]
        let relative = OsString::from_wide(&[0x73, 0x74, 0x61, 0x74, 0x65, 0x2d, 0xd800]);
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        let paths = [config::ConfigPath::File(path, None)];
        let expected = std::env::current_dir().unwrap().join(&relative);
        for data_dir in [Path::new(&relative), expected.as_path()] {
            let args = create_service_arguments(&paths, Some(data_dir)).unwrap();
            assert_eq!(args.last().unwrap(), expected.as_os_str());
            let parsed = crate::cli::RootOpts::try_parse_from(
                std::iter::once(OsString::from("vector")).chain(args),
            )
            .unwrap();
            assert_eq!(parsed.data_dir.as_ref(), Some(&expected));
        }
    }

    #[test]
    fn service_data_dir_rejects_empty_override() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        assert!(
            create_service_arguments(&[config::ConfigPath::File(path, None)], Some(Path::new("")),)
                .is_none()
        );
    }

    #[cfg(windows)]
    #[test]
    fn service_data_dir_resolves_windows_relative_forms() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        let paths = [config::ConfigPath::File(path, None)];
        for relative in [
            r".\state with spaces\",
            r"\state with spaces\",
            r"C:state with spaces\",
        ] {
            let expected = std::path::absolute(relative).unwrap();
            let args = create_service_arguments(&paths, Some(Path::new(relative))).unwrap();
            assert!(Path::new(args.last().unwrap()).is_absolute());
            assert_eq!(args.last().unwrap(), expected.as_os_str());
        }
    }

    #[cfg(windows)]
    #[test]
    fn service_data_dir_keeps_absolute_windows_paths_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("vector.yaml");
        write_config(&path, &directory.path().join("configured-state"), true);
        let paths = [config::ConfigPath::File(path, None)];
        for absolute in [
            r"C:\state with spaces\",
            r"\\server\share\state\",
            r"\\?\C:\state with spaces\",
        ] {
            let args = create_service_arguments(&paths, Some(Path::new(absolute))).unwrap();
            assert_eq!(args.last().unwrap(), absolute);
        }
    }
}

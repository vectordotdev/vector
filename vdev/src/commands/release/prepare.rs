use crate::git;
use crate::util::run_command;
use anyhow::Result;
use semver::Version;
use std::fs;
use std::path::Path;
use toml::map::Map;
use toml::Value;

/// Release preparations CLI options.
#[derive(clap::Args, Debug)]
#[command()]
pub struct Cli {
    /// The new Vector version.
    #[arg(long)]
    version: Version,
    /// The new VRL version.
    #[arg(long)]
    vrl_version: Version,
}

impl Cli {
    pub fn exec(self) -> Result<()> {
        create_release_branches(&self.version)?;
        pin_vrl_version(&self.vrl_version)?;

        // TODO automate more steps such as 'cargo vdev build release-cue'
        println!("Continue the release preparation process manually.");
        Ok(())
    }
}

/// Steps 1 & 2
fn create_release_branches(new_version: &Version) -> Result<()> {
    // Step 1: Create a new release branch
    git::run_and_check_output(&["fetch"])?;
    git::checkout_main_branch()?;
    let release_branch = format!("v{}.{}", new_version.major, new_version.minor);
    git::create_branch(release_branch.as_str())?;
    git::push_and_set_upstream(release_branch.as_str())?;

    // Step 2: Create a new release preparation branch
    //         The branch website contains 'website' to generate vector.dev preview.
    let release_preparation_branch = format!("website-prepare-v{new_version}");
    git::checkout_branch(release_preparation_branch.as_str())?;
    git::push_and_set_upstream(release_preparation_branch.as_str())?;
    Ok(())
}

/// Step 3
fn pin_vrl_version(new_version: &Version) -> Result<()> {
    let cargo_toml_path = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap().join("Cargo.toml");
    let contents = fs::read_to_string(&cargo_toml_path).expect("Failed to read Cargo.toml");

    // Needs this hybrid approach to preserve ordering.
    let mut lines: Vec<String> = contents.lines().map(String::from).collect();

    for line in &mut lines {
        if line.trim().starts_with("vrl = { git = ") {
            if let Ok(mut vrl_toml) = line.parse::<Value>() {
                let vrl_dependency: &mut Value = vrl_toml.get_mut("vrl").expect("line should start with 'vrl'");

                let mut new_dependency_value = Map::new();
                new_dependency_value.insert("version".to_string(), Value::String(new_version.to_string()));
                let features = vrl_dependency.get("features").expect("missing 'features' key");
                new_dependency_value.insert("features".to_string(), features.clone());

                *line = format!("vrl = {}", Value::from(new_dependency_value));
            }
            break;
        }
    }

    fs::write(&cargo_toml_path, lines.join("\n")).expect("Failed to write Cargo.toml");
    run_command("cargo update -p vrl");
    git::commit(&format!("chore(releasing): Pinned VRL version to {new_version}"))?;
    Ok(())
}

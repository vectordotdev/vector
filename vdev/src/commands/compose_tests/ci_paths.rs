use anyhow::Result;

use crate::testing::config::ComposeTestConfig;

pub(crate) fn exec(path: &str) -> Result<()> {
    // placeholder for changes that should run all integration tests
    println!("all-int: []");

    // paths for each integration are defined in their respective config files.
    for (integration, config) in ComposeTestConfig::collect_all(path)? {
        if let Some(paths) = config.paths {
            println!("{integration}:");
            for path in paths {
                println!("- \"{path}\"");
            }
        }
    }

    Ok(())
}

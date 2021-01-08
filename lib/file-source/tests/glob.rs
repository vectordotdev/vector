use file_source::paths_provider::{glob::Glob, PathsProvider};
use std::collections::HashSet;

#[test]
fn test_glob_include_plain() -> Result<(), Box<dyn std::error::Error>> {
    let include_patterns = ["tests/files/foo.log".to_owned()];
    let exclude_patterns = [];
    let glob = Glob::new(&include_patterns, &exclude_patterns)?;

    let paths = glob.paths()?;

    assert_eq!(
        paths.into_iter().collect::<HashSet<_>>(),
        ["./tests/files/foo.log"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<HashSet<_>>()
    );

    Ok(())
}

#[test]
fn test_glob_include_curly_braces() -> Result<(), Box<dyn std::error::Error>> {
    let include_patterns = ["tests/files/{foo,bar}.log".to_owned()];
    let exclude_patterns = [];
    let glob = Glob::new(&include_patterns, &exclude_patterns)?;

    let paths = glob.paths()?;

    assert_eq!(
        paths.into_iter().collect::<HashSet<_>>(),
        ["./tests/files/foo.log", "./tests/files/bar.log"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<HashSet<_>>()
    );

    Ok(())
}

#[test]
fn test_glob_include_curly_braces_exclude_star() -> Result<(), Box<dyn std::error::Error>> {
    let include_patterns = ["tests/files/{foo,bar}.log".to_owned()];
    let exclude_patterns = ["**/foo.log".to_owned()];
    let glob = Glob::new(&include_patterns, &exclude_patterns)?;

    let paths = glob.paths()?;

    assert_eq!(
        paths.into_iter().collect::<HashSet<_>>(),
        ["./tests/files/bar.log"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<HashSet<_>>()
    );

    Ok(())
}

#[test]
fn test_glob_include_invalid() -> Result<(), Box<dyn std::error::Error>> {
    let include_patterns = ["{{}".to_owned()];
    let exclude_patterns = [];
    let glob = Glob::new(&include_patterns, &exclude_patterns);

    assert!(glob.is_err());

    Ok(())
}

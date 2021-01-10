use file_source::paths_provider::{glob::Glob, PathsProvider};

fn sorted<T: Ord>(mut input: Vec<T>) -> Vec<T> {
    input.sort();
    input
}

#[test]
fn test_glob_include_plain() -> Result<(), Box<dyn std::error::Error>> {
    let include_patterns = ["tests/files/foo.log".to_owned()];
    let exclude_patterns = [];
    let glob = Glob::new(&include_patterns, &exclude_patterns)?;

    let paths = glob.paths()?;

    assert_eq!(
        paths,
        ["./tests/files/foo.log"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>()
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
        sorted(paths),
        ["./tests/files/foo.log", "./tests/files/bar.log"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>()
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
        paths,
        ["./tests/files/bar.log"]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>()
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

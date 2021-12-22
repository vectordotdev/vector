use std::path::Path;

use toml::{value::Map, Value};

use super::{
    loading::{component_name, load, open_file, read_dir},
    Format,
};

fn merge_values(value: toml::Value, other: toml::Value) -> Result<toml::Value, Vec<String>> {
    serde_toml_merge::merge(value, other).map_err(|err| vec![format!("{}", err)])
}

fn merge_with_value(
    res: &mut toml::map::Map<String, toml::Value>,
    name: String,
    value: toml::Value,
) -> Result<(), Vec<String>> {
    if let Some(existing) = res.remove(&name) {
        res.insert(name, merge_values(existing, value)?);
    } else {
        res.insert(name, value);
    }
    Ok(())
}

pub fn load_file(path: &Path) -> Result<Option<(String, toml::Value, Vec<String>)>, Vec<String>> {
    if let (Ok(name), Some(file), Ok(format)) = (
        component_name(path),
        open_file(path),
        Format::from_path(path),
    ) {
        load(file, format).map(|(value, warnings)| Some((name, value, warnings)))
    } else {
        Ok(None)
    }
}

pub fn load_file_recursive(
    path: &Path,
) -> Result<Option<(String, toml::Value, Vec<String>)>, Vec<String>> {
    if let Some((name, mut value, mut warnings)) = load_file(path)? {
        if let Some(subdir) = path.parent().map(|p| p.join(&name)) {
            if subdir.is_dir() && subdir.exists() {
                if let Some(table) = value.as_table_mut() {
                    warnings.extend(load_dir_into(&subdir, table)?);
                }
            }
        }
        Ok(Some((name, value, warnings)))
    } else {
        Ok(None)
    }
}

pub fn load_dir_into(
    path: &Path,
    result: &mut Map<String, Value>,
) -> Result<Vec<String>, Vec<String>> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();
    let readdir = read_dir(path)?;

    let mut files = Vec::new();
    let mut folders = Vec::new();

    for direntry in readdir {
        match direntry {
            Ok(item) => {
                let entry = item.path();
                if entry.is_file() {
                    files.push(entry);
                } else if entry.is_dir() {
                    folders.push(entry);
                }
            }
            Err(err) => {
                errors.push(format!(
                    "Could not read entry in config dir: {:?}, {}.",
                    path, err
                ));
            }
        };
    }

    for entry in files {
        match load_file_recursive(&entry) {
            Ok(Some((name, inner, warns))) => {
                if let Err(errs) = merge_with_value(result, name, inner) {
                    errors.extend(errs);
                } else {
                    warnings.extend(warns);
                }
            }
            Ok(None) => {}
            Err(errs) => {
                errors.extend(errs);
            }
        }
    }

    for entry in folders {
        if let Ok(name) = component_name(&entry) {
            if !result.contains_key(&name) {
                match load_dir(&entry) {
                    Ok((table, warns)) => {
                        result.insert(name, Value::Table(table));
                        warnings.extend(warns);
                    }
                    Err(errs) => {
                        errors.extend(errs);
                    }
                }
            }
        }
    }

    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(errors)
    }
}

pub fn load_dir(path: &Path) -> Result<(Map<String, Value>, Vec<String>), Vec<String>> {
    let mut result = Map::new();
    let warnings = load_dir_into(path, &mut result)?;
    Ok((result, warnings))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{load_dir, load_file_recursive};

    #[test]
    fn parse_dir_recursively() {
        let root = tempfile::tempdir().unwrap();
        let rootp = root.path();
        let foo = rootp.join("foo.toml");
        fs::write(&foo, "bar = 42").unwrap();
        let pipelines = rootp.join("foo").join("pipelines");
        fs::create_dir_all(&pipelines).unwrap();
        let first = pipelines.join("first.json");
        fs::write(
            &first,
            r#"
        {
            "name": "first",
            "transforms": [
                { "type": "noop" }
            ]
        }
        "#,
        )
        .unwrap();
        let (result, warnings) = load_dir(rootp).unwrap();
        assert!(warnings.is_empty());
        let expected: toml::Value = toml::from_str(
            r#"
        [foo]
        bar = 42
        [foo.pipelines.first]
        name = "first"

        [[foo.pipelines.first.transforms]]
        type = "noop"

        "#,
        )
        .unwrap();
        assert_eq!(toml::Value::Table(result), expected);
    }

    #[test]
    fn parse_file_and_subdir() {
        let root = tempfile::tempdir().unwrap();
        let rootp = root.path();
        let foo = rootp.join("foo.toml");
        fs::write(&foo, "bar = 42").unwrap();
        let pipelines = rootp.join("foo").join("pipelines");
        fs::create_dir_all(&pipelines).unwrap();
        let first = pipelines.join("first.json");
        fs::write(
            &first,
            r#"
        {
            "name": "first",
            "transforms": [
                { "type": "noop" }
            ]
        }
        "#,
        )
        .unwrap();
        let (name, result, warnings) = load_file_recursive(&foo).unwrap().unwrap();
        assert!(warnings.is_empty());
        assert_eq!(name, "foo");
        let expected: toml::Value = toml::from_str(
            r#"
        bar = 42
        [pipelines.first]
        name = "first"

        [[pipelines.first.transforms]]
        type = "noop"

        "#,
        )
        .unwrap();
        assert_eq!(result, expected);
    }

    #[test]
    fn overriding_existing_value() {
        let root = tempfile::tempdir().unwrap();
        let rootp = root.path();
        let foo = rootp.join("foo.toml");
        fs::write(
            &foo,
            r#"
            bar = 42

            [pipelines]
            first = "test"
        "#,
        )
        .unwrap();
        let pipelines = rootp.join("foo").join("pipelines");
        fs::create_dir_all(&pipelines).unwrap();
        let first = pipelines.join("first.json");
        fs::write(
            &first,
            r#"
        {
            "name": "first",
            "transforms": [
                { "type": "noop" }
            ]
        }
        "#,
        )
        .unwrap();
        let (name, result, warnings) = load_file_recursive(&foo).unwrap().unwrap();
        assert!(warnings.is_empty());
        assert_eq!(name, "foo");
        let expected: toml::Value = toml::from_str(
            r#"
        bar = 42
        [pipelines]
        first = "test"
        "#,
        )
        .unwrap();
        assert_eq!(result, expected);
    }
}

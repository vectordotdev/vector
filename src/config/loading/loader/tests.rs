use indoc::indoc;

use serde_json::{Value, json};

use super::{ConfigLoader, ConfigMap, ConfigPath, ConfigScope, Format, LoadMode, ParsedInputs};

#[derive(Default)]
struct RecordingLoader {
    roots: Vec<Value>,
    components: ConfigMap,
}

impl ConfigLoader for RecordingLoader {
    const MODE: LoadMode = LoadMode::Raw;

    fn merge(&mut self, map: ConfigMap, scope: ConfigScope) -> Result<(), Vec<String>> {
        match scope {
            ConfigScope::Root => self.roots.push(Value::Object(map)),
            ConfigScope::Component(_) => self.components = map,
        }
        Ok(())
    }
}

#[test]
fn repeated_file_paths_share_one_parsed_document() {
    const CONFIG: &str = indoc! {r"
        value: retained
    "};

    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shared.yaml");
    std::fs::write(&path, CONFIG).unwrap();
    let input = ConfigPath::File(path.clone(), Some(Format::Yaml));
    let parsed = ParsedInputs::from_paths(&[input.clone(), input]);

    assert_eq!(parsed.documents.len(), 1);
    assert_eq!(parsed.documents[0].path.as_ref(), Some(&path));
    assert_eq!(parsed.documents[0].format, Format::Yaml);
    let mut loader = RecordingLoader::default();
    parsed.load_into(&mut loader).unwrap();
    assert_eq!(loader.roots, vec![json!({"value": "retained"}); 2]);
    assert!(loader.components.is_empty());
}

#[test]
fn same_stem_files_share_their_nested_documents() {
    const YAML_PARENT: &str = indoc! {r"
        first: 1
    "};
    const JSON_PARENT: &str = r#"{"second":2}"#;
    const CHILD: &str = indoc! {r"
        nested: 3
    "};

    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let nested = transforms.join("parent");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(transforms.join("parent.yaml"), YAML_PARENT).unwrap();
    std::fs::write(transforms.join("parent.json"), JSON_PARENT).unwrap();
    std::fs::write(nested.join("child.yaml"), CHILD).unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    assert_eq!(parsed.documents.len(), 3);
    let mut loader = RecordingLoader::default();
    parsed.load_into(&mut loader).unwrap();
    // Sharing reads must not remove either file's contribution during assembly.
    assert_eq!(contributions_at(&parsed, &["parent", "child", "nested"]), 2);
    assert_eq!(
        Value::Object(loader.components),
        json!({"parent": {"first": 1, "second": 2, "child": {"nested": 3}}})
    );
}

#[test]
fn inline_transform_fields_prevent_reading_matching_folders() {
    const MAPPER: &str = indoc! {r"
        type: remap
        source: '.'
    "};
    const INVALID_CONFIG: &str = indoc! {r"
        not: [valid yaml
    "};

    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let ignored = transforms.join("mapper/source");
    std::fs::create_dir_all(&ignored).unwrap();
    let mapper = transforms.join("mapper.yaml");
    std::fs::write(&mapper, MAPPER).unwrap();
    std::fs::write(ignored.join("unread.yaml"), INVALID_CONFIG).unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    // Even a parse failure is retained when a file is read. Only mapper.yaml should
    // exist in the snapshot; the inline source field hides the entire source folder.
    assert_eq!(parsed.documents.len(), 1);
    assert_eq!(parsed.documents[0].path.as_ref(), Some(&mapper));
    let mut loader = RecordingLoader::default();
    parsed.load_into(&mut loader).unwrap();
    assert_eq!(
        Value::Object(loader.components),
        json!({"mapper": {"type": "remap", "source": "."}})
    );
}

#[test]
fn same_stem_files_apply_their_own_inline_folder_masks() {
    const YAML_PARENT: &str = indoc! {r"
        child:
          inline: true
    "};
    const JSON_PARENT: &str = r#"{"other":true}"#;
    const NESTED: &str = indoc! {r"
        nested: true
    "};

    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let nested = transforms.join("parent/child");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(transforms.join("parent.yaml"), YAML_PARENT).unwrap();
    std::fs::write(transforms.join("parent.json"), JSON_PARENT).unwrap();
    std::fs::write(nested.join("disk.yaml"), NESTED).unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    assert_eq!(parsed.documents.len(), 3);
    let mut loader = RecordingLoader::default();
    parsed.load_into(&mut loader).unwrap();
    assert_eq!(
        contributions_at(&parsed, &["parent", "child", "disk", "nested"]),
        1
    );
    assert_eq!(
        Value::Object(loader.components),
        json!({"parent": {"child": {"inline": true, "disk": {"nested": true}}, "other": true}})
    );
}

#[test]
fn inline_fields_do_not_suppress_nested_files() {
    const PARENT: &str = indoc! {r"
        child:
          inline: true
    "};
    const CHILD: &str = indoc! {r"
        disk: true
    "};

    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let nested = transforms.join("parent");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(transforms.join("parent.yaml"), PARENT).unwrap();
    std::fs::write(nested.join("child.yaml"), CHILD).unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    assert_eq!(parsed.documents.len(), 2);
    let mut loader = RecordingLoader::default();
    parsed.load_into(&mut loader).unwrap();
    assert_eq!(
        Value::Object(loader.components),
        json!({"parent": {"child": {"inline": true, "disk": true}}})
    );
}

#[cfg(unix)]
#[test]
fn directory_cycles_only_fail_when_assembly_reaches_them() {
    const INLINE_PARENT: &str = indoc! {r"
        blocked:
          source: inline
    "};
    const EMPTY_PARENT: &str = indoc! {r"
        {}
    "};

    for ignored in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let transforms = temp.path().join("transforms");
        let nested = transforms.join("parent");
        std::fs::create_dir_all(&nested).unwrap();
        let parent = if ignored { INLINE_PARENT } else { EMPTY_PARENT };
        std::fs::write(transforms.join("parent.yaml"), parent).unwrap();
        std::os::unix::fs::symlink(&nested, nested.join("blocked")).unwrap();
        let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);
        let mut loader = RecordingLoader::default();
        let result = parsed.load_into(&mut loader);

        if ignored {
            assert!(
                parsed
                    .inputs
                    .iter()
                    .flatten()
                    .all(|batch| batch.tree.is_ok())
            );
            result.unwrap();
            assert_eq!(
                Value::Object(loader.components),
                json!({"parent": {"blocked": {"source": "inline"}}})
            );
        } else {
            let errors = result.unwrap_err();
            assert!(!errors.is_empty());
            assert!(
                errors
                    .iter()
                    .all(|error| error.contains("symbolic-link cycle"))
            );
        }
    }
}

fn contributions_at(parsed: &ParsedInputs, path: &[&str]) -> usize {
    parsed
        .inputs
        .iter()
        .flatten()
        .filter_map(|batch| {
            path.iter()
                .try_fold(batch.tree.as_ref().ok()?, |node, key| {
                    node.fields().get(*key)
                })
        })
        .map(|node| match node {
            super::ConfigNode::Scalar(values) => values.len(),
            _ => panic!("expected scalar contribution"),
        })
        .sum()
}

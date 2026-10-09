use serde_json::{Value, json};

use super::{ConfigMap, ConfigPath, ConfigScope, Format, ParsedInputs};

#[test]
fn repeated_file_paths_share_one_parsed_document() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shared.yaml");
    std::fs::write(&path, "value: retained\n").unwrap();
    let input = ConfigPath::File(path.clone(), Some(Format::Yaml));
    let parsed = ParsedInputs::from_paths(&[input.clone(), input]);

    assert_eq!(parsed.documents.len(), 1);
    assert_eq!(parsed.documents[0].origin.path.as_ref(), Some(&path));
    assert_eq!(parsed.documents[0].origin.format, Format::Yaml);
    let mut maps = Vec::new();
    parsed
        .assemble(
            |_| {},
            |map, scope| {
                assert!(matches!(scope, ConfigScope::Root));
                maps.push(Value::Object(map));
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(maps, vec![json!({"value": "retained"}); 2]);
}

#[test]
fn same_stem_files_share_their_nested_documents() {
    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let nested = transforms.join("parent");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(transforms.join("parent.yaml"), "first: 1\n").unwrap();
    std::fs::write(transforms.join("parent.json"), r#"{"second":2}"#).unwrap();
    std::fs::write(nested.join("child.yaml"), "nested: 3\n").unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    assert_eq!(parsed.documents.len(), 3);
    let mut nested_visits = 0;
    let mut components = ConfigMap::new();
    parsed
        .assemble(
            |document| {
                if document.contains_key("nested") {
                    nested_visits += 1;
                }
            },
            |map, scope| {
                if matches!(scope, ConfigScope::Component(_)) {
                    components = map;
                }
                Ok(())
            },
        )
        .unwrap();
    // Sharing reads must not remove either file's contribution during assembly.
    assert_eq!(nested_visits, 2);
    assert_eq!(
        Value::Object(components),
        json!({"parent": {"first": 1, "second": 2, "child": {"nested": 3}}})
    );
}

#[test]
fn inline_transform_fields_prevent_reading_matching_folders() {
    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let ignored = transforms.join("mapper/source");
    std::fs::create_dir_all(&ignored).unwrap();
    let mapper = transforms.join("mapper.yaml");
    std::fs::write(&mapper, "type: remap\nsource: '.'\n").unwrap();
    std::fs::write(ignored.join("unread.yaml"), "not: [valid yaml\n").unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    // Even a parse failure is retained when a file is read. Only mapper.yaml should
    // exist in the snapshot; the inline source field hides the entire source folder.
    assert_eq!(parsed.documents.len(), 1);
    assert_eq!(parsed.documents[0].origin.path.as_ref(), Some(&mapper));
    let mut components = ConfigMap::new();
    parsed
        .assemble(
            |_| {},
            |map, scope| {
                if matches!(scope, ConfigScope::Component(_)) {
                    components = map;
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(
        Value::Object(components),
        json!({"mapper": {"type": "remap", "source": "."}})
    );
}

#[test]
fn same_stem_files_apply_their_own_inline_folder_masks() {
    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let nested = transforms.join("parent/child");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(transforms.join("parent.yaml"), "child:\n  inline: true\n").unwrap();
    std::fs::write(transforms.join("parent.json"), r#"{"other":true}"#).unwrap();
    std::fs::write(nested.join("disk.yaml"), "nested: true\n").unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    assert_eq!(parsed.documents.len(), 3);
    let mut nested_visits = 0;
    let mut components = ConfigMap::new();
    parsed
        .assemble(
            |document| {
                if document.contains_key("nested") {
                    nested_visits += 1;
                }
            },
            |map, scope| {
                if matches!(scope, ConfigScope::Component(_)) {
                    components = map;
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(nested_visits, 1);
    assert_eq!(
        Value::Object(components),
        json!({"parent": {"child": {"inline": true, "disk": {"nested": true}}, "other": true}})
    );
}

#[test]
fn inline_fields_do_not_suppress_nested_files() {
    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let nested = transforms.join("parent");
    std::fs::create_dir_all(&nested).unwrap();
    std::fs::write(transforms.join("parent.yaml"), "child:\n  inline: true\n").unwrap();
    std::fs::write(nested.join("child.yaml"), "disk: true\n").unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    assert_eq!(parsed.documents.len(), 2);
    let mut components = ConfigMap::new();
    parsed
        .assemble(
            |_| {},
            |map, scope| {
                if matches!(scope, ConfigScope::Component(_)) {
                    components = map;
                }
                Ok(())
            },
        )
        .unwrap();
    assert_eq!(
        Value::Object(components),
        json!({"parent": {"child": {"inline": true, "disk": true}}})
    );
}

#[cfg(unix)]
#[test]
fn directory_cycles_only_fail_when_assembly_reaches_them() {
    for ignored in [true, false] {
        let temp = tempfile::tempdir().unwrap();
        let transforms = temp.path().join("transforms");
        let nested = transforms.join("parent");
        std::fs::create_dir_all(&nested).unwrap();
        let parent = if ignored {
            "blocked:\n  source: inline\n"
        } else {
            "{}\n"
        };
        std::fs::write(transforms.join("parent.yaml"), parent).unwrap();
        std::os::unix::fs::symlink(&nested, nested.join("blocked")).unwrap();
        let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);
        let mut components = ConfigMap::new();
        let result = parsed.assemble(
            |_| {},
            |map, scope| {
                if matches!(scope, ConfigScope::Component(_)) {
                    components = map;
                }
                Ok(())
            },
        );

        if ignored {
            assert!(parsed.directories.iter().all(Result::is_ok));
            result.unwrap();
            assert_eq!(
                Value::Object(components),
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

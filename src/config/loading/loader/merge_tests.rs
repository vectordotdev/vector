use std::collections::HashMap;

use indoc::indoc;
use serde_json::{Value, json};

use super::{ConfigPath, InputReader, ParsedInputs};
use crate::config::loading::{
    ConfigBuilderLoader, SecretBackendLoader, SourceLoader, interpolation::interpolate,
};

fn count_fragments(namespaced: bool, values: &[Value], reversed: bool) -> ParsedInputs {
    const COUNT_FRAGMENT: &str = r#"{"count":null}"#;
    const SOURCE_FRAGMENT: &str = indoc! {r#"
        {
          "type": "demo_logs",
          "format": "json",
          "count": null
        }
    "#};
    const ROOT_FRAGMENT: &str = indoc! {r#"
        {
          "sources": {
            "demo": {"count": null}
          }
        }
    "#};

    const COMPONENT_KIND: &str = indoc! {"
        sources:
          demo:
            type: demo_logs
            format: json
    "};

    let temp = tempfile::tempdir().unwrap();
    let directory = if namespaced {
        temp.path().join("sources")
    } else {
        temp.path().to_owned()
    };
    std::fs::create_dir_all(&directory).unwrap();
    for (index, value) in values.iter().enumerate() {
        let template = match (namespaced, index) {
            (true, 0) => SOURCE_FRAGMENT,
            (true, _) => COUNT_FRAGMENT,
            (false, _) => ROOT_FRAGMENT,
        };
        let mut document: Value = serde_json::from_str(template).unwrap();
        let component = if namespaced {
            &mut document
        } else {
            &mut document["sources"]["demo"]
        };
        component["count"] = value.clone();
        let extension = ["yaml", "yml", "json"][index];
        std::fs::write(
            directory.join(format!("demo.{extension}")),
            serde_json::to_string(&document).unwrap(),
        )
        .unwrap();
    }
    if !namespaced {
        // The type tag and required format are in another root file, not the
        // same-stem group whose integer/string conflict needs normalization.
        std::fs::write(directory.join("kind.yaml"), COMPONENT_KIND).unwrap();
    }
    ordered_inputs(temp.path(), reversed)
}

fn ordered_inputs(root: &std::path::Path, reversed: bool) -> ParsedInputs {
    let mut reader = InputReader::default();
    for directory in [root.to_owned(), root.join("sources")] {
        if !directory.is_dir() {
            continue;
        }
        let mut listing = InputReader::list_directory(&directory).unwrap();
        listing
            .files
            .sort_by_key(|path| match path.extension().and_then(|ext| ext.to_str()) {
                Some("yaml") => 0,
                Some("yml") => 1,
                Some("json") => 2,
                _ => unreachable!("test uses only YAML and JSON files"),
            });
        if reversed {
            listing.files.reverse();
        }
        reader.listings.insert(directory, Ok(listing));
    }
    reader.read_paths(&[ConfigPath::Dir(root.to_owned())])
}

#[test]
fn same_stem_interpolated_counts_use_the_complete_component() {
    for namespaced in [false, true] {
        for reversed in [false, true] {
            let mut parsed = count_fragments(namespaced, &[json!(2), json!("${COUNT}")], reversed);
            parsed.interpolate_strings(|value| {
                interpolate(
                    value,
                    &HashMap::from([("COUNT".to_owned(), "2".to_owned())]),
                )
            });
            // Raw source assembly must retain its original strict type rules.
            assert!(parsed.load_into(&mut SourceLoader::new()).is_err());
            SecretBackendLoader::default()
                .load_prepared(&parsed)
                .unwrap();
            let builder = ConfigBuilderLoader::default()
                .load_prepared(&parsed)
                .unwrap_or_else(|errors| {
                    panic!("namespaced={namespaced}, reversed={reversed}: {errors:?}")
                });
            assert_eq!(
                serde_json::to_value(builder).unwrap()["sources"]["demo"]["count"],
                2
            );
        }
    }
}

#[test]
fn same_stem_invalid_values_cannot_be_hidden_by_later_strings() {
    for namespaced in [false, true] {
        for values in [
            vec![json!(2), json!("bad"), json!("2")],
            vec![json!("bad"), json!("2"), json!(2)],
            vec![json!("bad"), json!("2")],
            vec![json!("1"), json!("bad"), json!("2")],
        ] {
            for reversed in [false, true] {
                let parsed = count_fragments(namespaced, &values, reversed);
                let errors = ConfigBuilderLoader::default()
                    .load_prepared(&parsed)
                    .unwrap_err();
                assert!(
                    errors
                        .iter()
                        .any(|error| error.contains("sources.demo.count")),
                    "namespaced={namespaced}, reversed={reversed}, values={values:?}: {errors:?}"
                );
            }
        }
    }
}

#[test]
fn same_stem_global_boolean_conflicts_are_resolved_after_assembly() {
    const BOOLEAN_PROXY: &str = indoc! {"
        proxy:
          enabled: true
    "};
    const STRING_PROXY: &str = indoc! {"
        proxy:
          enabled: 'true'
    "};

    for reversed in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("proxy.yaml"), BOOLEAN_PROXY).unwrap();
        std::fs::write(temp.path().join("proxy.yml"), STRING_PROXY).unwrap();
        let parsed = ordered_inputs(temp.path(), reversed);
        SecretBackendLoader::default()
            .load_prepared(&parsed)
            .unwrap();
        let builder = ConfigBuilderLoader::default()
            .load_prepared(&parsed)
            .unwrap();
        assert!(builder.global.proxy.enabled);
    }
}

#[cfg(feature = "transforms-remap")]
#[test]
fn deferred_same_stem_merges_do_not_read_inline_shadowed_folders() {
    const BOOLEAN_MAPPER: &str = indoc! {"
        type: remap
        inputs: []
        source: '.'
        drop_on_error: true
    "};
    const STRING_MAPPER: &str = indoc! {"
        source: '.'
        drop_on_error: 'true'
    "};
    const INVALID_CONFIG: &str = indoc! {"
        invalid: [yaml
    "};

    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let ignored = transforms.join("mapper/source");
    std::fs::create_dir_all(&ignored).unwrap();
    std::fs::write(transforms.join("mapper.yaml"), BOOLEAN_MAPPER).unwrap();
    std::fs::write(transforms.join("mapper.yml"), STRING_MAPPER).unwrap();
    std::fs::write(ignored.join("unread.yaml"), INVALID_CONFIG).unwrap();
    let parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);

    assert_eq!(parsed.documents.len(), 2);
    let builder = ConfigBuilderLoader::default()
        .load_prepared(&parsed)
        .unwrap();
    assert_eq!(
        serde_json::to_value(builder).unwrap()["transforms"]["mapper"]["drop_on_error"],
        true
    );
}

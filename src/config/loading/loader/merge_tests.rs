use std::collections::HashMap;

use serde_json::{Value, json};

use super::{ConfigPath, ParsedInputs};
use crate::config::loading::{
    ConfigBuilderLoader, SecretBackendLoader, interpolate_config_map_with_env_vars,
};

fn count_fragments(namespaced: bool, values: &[Value]) -> ParsedInputs {
    let temp = tempfile::tempdir().unwrap();
    let directory = if namespaced {
        temp.path().join("sources")
    } else {
        temp.path().to_owned()
    };
    std::fs::create_dir_all(&directory).unwrap();
    for (index, value) in values.iter().enumerate() {
        let mut component = json!({"count": value});
        if namespaced && index == 0 {
            component["type"] = json!("demo_logs");
            component["format"] = json!("json");
        }
        let document = if namespaced {
            component
        } else {
            json!({"sources": {"demo": component}})
        };
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
        std::fs::write(
            directory.join("kind.yaml"),
            "sources:\n  demo:\n    type: demo_logs\n    format: json\n",
        )
        .unwrap();
    }
    let mut parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);
    let documents = &parsed.documents;
    for directory in parsed.directories.iter_mut().flatten() {
        directory.files.sort_by_key(|file| {
            match documents[file.document]
                .origin
                .path
                .as_ref()
                .and_then(|path| path.extension())
                .and_then(|extension| extension.to_str())
            {
                Some("yaml") => 0,
                Some("yml") => 1,
                Some("json") => 2,
                _ => unreachable!("test uses only YAML and JSON files"),
            }
        });
    }
    parsed
}

fn reverse_files(parsed: &mut ParsedInputs) {
    for directory in parsed.directories.iter_mut().flatten() {
        directory.files.reverse();
    }
}

#[test]
fn same_stem_interpolated_counts_use_the_complete_component() {
    for namespaced in [false, true] {
        for reversed in [false, true] {
            let mut parsed = count_fragments(namespaced, &[json!(2), json!("${COUNT}")]);
            parsed.update_documents(|map| {
                interpolate_config_map_with_env_vars(
                    map,
                    &HashMap::from([("COUNT".to_owned(), "2".to_owned())]),
                )
            });
            if reversed {
                reverse_files(&mut parsed);
            }
            // Raw source assembly must retain its original strict type rules.
            assert!(parsed.assemble(|_| {}, |_, _| Ok(())).is_err());
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
        ] {
            for reversed in [false, true] {
                let mut parsed = count_fragments(namespaced, &values);
                if reversed {
                    reverse_files(&mut parsed);
                }
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
    for reversed in [false, true] {
        let temp = tempfile::tempdir().unwrap();
        std::fs::write(temp.path().join("proxy.yaml"), "proxy:\n  enabled: true\n").unwrap();
        std::fs::write(temp.path().join("proxy.yml"), "proxy:\n  enabled: 'true'\n").unwrap();
        let mut parsed = ParsedInputs::from_paths(&[ConfigPath::Dir(temp.path().to_owned())]);
        if reversed {
            reverse_files(&mut parsed);
        }
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
    let temp = tempfile::tempdir().unwrap();
    let transforms = temp.path().join("transforms");
    let ignored = transforms.join("mapper/source");
    std::fs::create_dir_all(&ignored).unwrap();
    std::fs::write(
        transforms.join("mapper.yaml"),
        "type: remap\ninputs: []\nsource: '.'\ndrop_on_error: true\n",
    )
    .unwrap();
    std::fs::write(
        transforms.join("mapper.yml"),
        "source: '.'\ndrop_on_error: 'true'\n",
    )
    .unwrap();
    std::fs::write(ignored.join("unread.yaml"), "invalid: [yaml\n").unwrap();
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

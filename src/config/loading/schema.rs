use super::ConfigBuilder;

fn into_errors<E: std::fmt::Debug>(err: E) -> Vec<String> {
    vec![format!("{:?}", err)]
}

fn sensitive_string_format(value: &str) -> bool {
    dbg!(value);
    false
}

fn generate_schema() -> Result<serde_json::Value, Vec<String>> {
    let schema =
        vector_config::schema::generate_root_schema::<ConfigBuilder>().map_err(into_errors)?;

    let mut schema = serde_json::to_value(&schema).map_err(into_errors)?;
    // add format key to enforce check
    if let Some(def) = schema
        .as_object_mut()
        .and_then(|r| r.get_mut("definitions"))
        .and_then(|d| d.as_object_mut())
        .and_then(|d| d.get_mut("vector_common::sensitive_string::SensitiveString"))
        .and_then(|d| d.as_object_mut())
    {
        def.insert("format".into(), serde_json::json!("sensitive_string"));

        Ok(schema)
    } else {
        Err(vec!["Unable to get SensitiveString definition".into()])
    }
}

pub(crate) fn check_sensitive_fields(json_config: &str) -> Result<(), Vec<String>> {
    let json_value = serde_json::from_str(json_config).map_err(into_errors)?;
    let schema = generate_schema()?;

    // compile the json schema with the custom format for sensitive strings
    let compiled = jsonschema::JSONSchema::options()
        .with_format("sensitive_string", sensitive_string_format)
        .should_validate_formats(true)
        .compile(&schema)
        .map_err(into_errors)?;

    compiled.validate(&json_value).map_err(|errors| {
        errors
            .map(|err| {
                format!(
                    "validation error ({}) as instance path {}",
                    err, err.instance_path
                )
            })
            .collect::<Vec<String>>()
    })
}

#[cfg(test)]
mod tests {
    use super::check_sensitive_fields;

    #[test]
    fn should_detect_api_keys() {
        let config = r#"{
    "enterprise": {
        "enabled": true,
        "api_key": "aaaaa",
        "configuration_key": "bbbbb"
    },
    "sources": {
        "foo": {
            "type": "demo_logs",
            "format": "json",
            "interval": 0.2
        }
    },
    "sinks": {
        "byebye": {
            "type": "blackhole",
            "inputs": ["foo"]
        }
    }
}"#;
        let errors = check_sensitive_fields(config).unwrap_err();
        assert_eq!(errors[0], "oops");
    }
}

use super::ConfigBuilder;

const SECRET_AND_VARIABLES_REGEX: &str = "(:?\\$\\{.+\\})";

fn into_errors<E: std::fmt::Debug>(err: E) -> Vec<String> {
    vec![format!("{:?}", err)]
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
        def.insert(
            "pattern".into(),
            serde_json::json!(SECRET_AND_VARIABLES_REGEX),
        );

        Ok(schema)
    } else {
        Err(vec!["Unable to get SensitiveString definition".into()])
    }
}

fn serialize_validation_errors(errors: jsonschema::ErrorIterator) -> Vec<String> {
    errors
        .map(|err| format!("{} at instance path {}", err, err.instance_path))
        .collect::<Vec<String>>()
}

pub(crate) fn check_sensitive_fields(json_config: &str) -> Result<(), Vec<String>> {
    let json_value = serde_json::from_str(json_config).map_err(into_errors)?;
    let schema = generate_schema()?;

    // compile the json schema with the custom format for sensitive strings
    let compiled = jsonschema::JSONSchema::options()
        .should_validate_formats(true)
        .compile(&schema)
        .map_err(into_errors)?;

    compiled
        .validate(&json_value)
        .map_err(serialize_validation_errors)
}

#[cfg(test)]
mod tests {
    use super::check_sensitive_fields;
    use regex::Regex;

    #[test]
    fn regex_should_detect_variables() {
        let re = Regex::new(super::SECRET_AND_VARIABLES_REGEX).unwrap();
        assert!(re.is_match("oewifn${foiwenf}wwefgno"));
        assert!(!re.is_match("oewifn${wwefgno"));
        assert!(!re.is_match("oewifn$}wwefgno"));
    }

    #[test]
    fn schema_should_detect_missing_variable_in_keys() {
        let config = r#"{
    "enterprise": {
        "api_key": "aaaaa",
        "configuration_key": "bbbbb"
    },
    "sources": {},
    "sinks": {}
}"#;
        let errors = check_sensitive_fields(config).unwrap_err();
        assert_eq!(errors[0], "{\"api_key\":\"aaaaa\",\"configuration_key\":\"bbbbb\"} is not valid under any of the given schemas at instance path /enterprise");
    }
}

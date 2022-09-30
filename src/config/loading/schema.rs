use super::ConfigBuilder;
use crate::config;
use vector_common::sensitive_string::SensitiveString;
use vector_config::Configurable;

const SECRET_AND_VARIABLES_REGEX: &str =
    "(:?\\$\\{.+\\})|(:?SECRET\\[([[:word:]]+)\\.([[:word:].]+)\\])";

fn into_errors<E: std::fmt::Debug>(err: E) -> Vec<String> {
    vec![format!("{:?}", err)]
}

fn generate_schema() -> Result<serde_json::Value, Vec<String>> {
    let schema =
        vector_config::schema::generate_root_schema::<ConfigBuilder>().map_err(into_errors)?;

    let mut schema = serde_json::to_value(&schema).map_err(into_errors)?;
    // add format key to enforce check
    let name = <SensitiveString as Configurable>::referenceable_name()
        .expect("Could not get SensitiveString referenceable name.");
    let def = schema
        .as_object_mut()
        .and_then(|r| r.get_mut("definitions"))
        .and_then(|d| d.as_object_mut())
        .and_then(|d| d.get_mut(name))
        .and_then(|d| d.as_object_mut())
        .expect("Could not get SensitiveString definition.");
    def.insert(
        "pattern".into(),
        serde_json::json!(SECRET_AND_VARIABLES_REGEX),
    );

    Ok(schema)
}

fn serialize_validation_errors(errors: jsonschema::ErrorIterator) -> Vec<String> {
    errors
        .map(|err| format!("{} at instance path {}", err, err.instance_path))
        .collect::<Vec<String>>()
}

fn check_sensitive_fields(json_config: &str) -> Result<(), Vec<String>> {
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

pub(crate) fn check_sensitive_fields_from_string(
    json: &str,
    builder: &crate::config::ConfigBuilder,
) -> Result<(), Vec<String>> {
    if builder
        .enterprise
        .as_ref()
        .map(|opts| opts.enabled)
        .unwrap_or_default()
    {
        debug!("Checking environment variables are used in sensitive strings.");
        check_sensitive_fields(json)?;
    }

    Ok(())
}

pub(crate) fn check_sensitive_fields_from_paths(
    paths: &[crate::config::ConfigPath],
    builder: &crate::config::ConfigBuilder,
) -> Result<(), Vec<String>> {
    let (source, _) = config::load_source_from_paths(paths)?;

    let json = config::util::json::serialize(source, builder, false, false);
    let json = json.expect("config should be serializable");

    check_sensitive_fields_from_string(&json, builder)
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
    fn regex_should_detect_secrets() {
        let re = Regex::new(super::SECRET_AND_VARIABLES_REGEX).unwrap();
        assert!(re.is_match("SECRET[a.secret.key]"));
        assert!(re.is_match("SECRET[a.secret.key] SECRET[a.secret.key]"));
        assert!(re.is_match("xxxSECRET[a.secret.key]yyy"));
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

    #[test]
    fn schema_should_detect_secrets() {
        let config = r#"{
    "enterprise": {
        "api_key": "SECRET[foo.bar]",
        "configuration_key": "SECRET[foo.baz]"
    },
    "sources": {},
    "sinks": {}
}"#;
        check_sensitive_fields(config).unwrap();
    }
}

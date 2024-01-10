use serde_json::Value;
use vector_config_common::{
    constants,
    human_friendly::generate_human_friendly_string,
    schema::{visit::Visitor, *},
};

/// A visitor that generates a human-friendly name for enum variants and fields as metadata.
///
/// Generally, we rely on rich documentation to provide human-friendly descriptions of types and
/// fields, but there is no such mechanism to provide a human-friendly name for types and fields
/// directly from their documentation comments. While it is possible to do so with manual metadata
/// annotations, it is laborious and prone to error.
///
/// This visitor generates a human-friendly name for types and fields, stored in metadata
/// (`docs::human_name`) using a simple set of heuristics to figure out how to break apart
/// type/field names, as well as what the case of each word should be, including accommodations for
/// well-known technical terms/acronyms, and so on.
///
/// ## Opting out of the visitor behavior
///
/// This approach has a very high hit rate, as the corpus we're operating on is generally small and
/// well contained, leading to requiring only a small set of replacements and logic. However, for
/// cases when this approach is not suitable, upstream usages can declare `docs::human_name`
/// themselves. Whenever the visitor sees that the metadata annotation is already present, it will
/// skip generating it.
#[derive(Debug, Default)]
pub struct GenerateHumanFriendlyNameVisitor;

impl GenerateHumanFriendlyNameVisitor {
    pub fn from_settings(_: &SchemaSettings) -> Self {
        Self
    }
}

impl Visitor for GenerateHumanFriendlyNameVisitor {
    fn visit_schema_object(
        &mut self,
        definitions: &mut Map<String, Schema>,
        schema: &mut SchemaObject,
    ) {
        // Recursively visit this schema first.
        visit::visit_schema_object(self, definitions, schema);

        // Skip this schema if it already has a human-friendly name defined.
        if has_schema_metadata_attr_str(schema, constants::DOCS_META_HUMAN_NAME) {
            return;
        }

        // When a logical name (via `logical_name`) is present, we use that as the source for
        // generating the human-friendly name. Logical name is populated for schemas that represent
        // an enum variant.
        if let Some(logical_name) = get_schema_metadata_attr_str(schema, constants::LOGICAL_NAME) {
            let human_name = generate_human_friendly_string(logical_name);
            set_schema_metadata_attr_str(schema, constants::DOCS_META_HUMAN_NAME, human_name);
        }

        // If the schema has object properties, we'll individually add the human name to each
        // property's schema if it doesn't already have a human-friendly name defined.
        if let Some(properties) = schema.object.as_mut().map(|object| &mut object.properties) {
            for (property_name, property_schema) in properties.iter_mut() {
                if let Some(property_schema) = property_schema.as_object_mut() {
                    if !has_schema_metadata_attr_str(
                        property_schema,
                        constants::DOCS_META_HUMAN_NAME,
                    ) {
                        let human_name = generate_human_friendly_string(property_name);
                        set_schema_metadata_attr_str(
                            property_schema,
                            constants::DOCS_META_HUMAN_NAME,
                            human_name,
                        );
                    }
                }
            }
        }
    }
}

fn has_schema_metadata_attr_str(schema: &SchemaObject, key: &str) -> bool {
    get_schema_metadata_attr_str(schema, key).is_some()
}

fn get_schema_metadata_attr_str<'a>(schema: &'a SchemaObject, key: &str) -> Option<&'a str> {
    schema
        .extensions
        .get(constants::METADATA)
        .and_then(|metadata| metadata.get(key))
        .and_then(|value| value.as_str())
}

fn set_schema_metadata_attr_str(schema: &mut SchemaObject, key: &str, value: String) {
    let metadata = schema
        .extensions
        .entry(constants::METADATA.to_string())
        .or_insert_with(|| Value::Object(serde_json::Map::new()));

    let metadata_map = metadata
        .as_object_mut()
        .expect("schema metadata must always be an object");
    metadata_map.insert(key.to_string(), Value::String(value));
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use vector_config_common::schema::visit::Visitor;

    use crate::schema::visitors::test::{as_schema, assert_schemas_eq};

    use super::GenerateHumanFriendlyNameVisitor;

    #[test]
    fn logical_name() {
        let mut actual_schema = as_schema(json!({
            "type": "string",
            "_metadata": {
                "logical_name": "LogToMetric"
            }
        }));

        let expected_schema = as_schema(json!({
            "type": "string",
            "_metadata": {
                "docs::human_name": "Log To Metric",
                "logical_name": "LogToMetric"
            }
        }));

        let mut visitor = GenerateHumanFriendlyNameVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn logical_name_with_replacement() {
        let mut actual_schema = as_schema(json!({
            "type": "string",
            "_metadata": {
                "logical_name": "AwsCloudwatchLogs"
            }
        }));

        let expected_schema = as_schema(json!({
            "type": "string",
            "_metadata": {
                "docs::human_name": "AWS CloudWatch Logs",
                "logical_name": "AwsCloudwatchLogs"
            }
        }));

        let mut visitor = GenerateHumanFriendlyNameVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn property_name() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "store_key": { "type": "boolean" }
            }
        }));

        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "store_key": {
                    "type": "boolean",
                    "_metadata": {
                      "docs::human_name": "Store Key"
                    }
                }
            }
        }));

        let mut visitor = GenerateHumanFriendlyNameVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn property_name_with_replacement() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "store_api_key": { "type": "boolean" }
            }
        }));

        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "store_api_key": {
                    "type": "boolean",
                    "_metadata": {
                      "docs::human_name": "Store API Key"
                    }
                }
            }
        }));

        let mut visitor = GenerateHumanFriendlyNameVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn logical_name_override() {
        let mut actual_schema = as_schema(json!({
            "type": "string",
            "_metadata": {
                "docs::human_name": "AWS EC2 Metadata",
                "logical_name": "Ec2Metadata"
            }
        }));

        let expected_schema = actual_schema.clone();

        let mut visitor = GenerateHumanFriendlyNameVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn property_name_override() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "store_api_key": {
                    "type": "boolean",
                    "_metadata": {
                        "docs::human_name": "Store_api_key"
                    }
                }
            }
        }));

        let expected_schema = actual_schema.clone();

        let mut visitor = GenerateHumanFriendlyNameVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }

    #[test]
    fn mixed_with_replacement() {
        let mut actual_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "store_api_key": { "type": "boolean" }
            },
            "_metadata": {
                "logical_name": "AwsEc2Metadata"
            }
        }));

        let expected_schema = as_schema(json!({
            "type": "object",
            "properties": {
                "store_api_key": {
                    "type": "boolean",
                    "_metadata": {
                      "docs::human_name": "Store API Key"
                    }
                }
            },
            "_metadata": {
                "docs::human_name": "AWS EC2 Metadata",
                "logical_name": "AwsEc2Metadata"
            }
        }));

        let mut visitor = GenerateHumanFriendlyNameVisitor;
        visitor.visit_root_schema(&mut actual_schema);

        assert_schemas_eq(expected_schema, actual_schema);
    }
}

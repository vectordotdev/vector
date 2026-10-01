use super::{SchemaContext, docs_type_str, get_schema_metadata, nested_merge};
use anyhow::{Result, bail};
use serde_json::{Value, json};

impl SchemaContext {
    pub fn resolve_schema_by_name(&mut self, schema_name: &str) -> Result<Value> {
        if let Some(resolved) = self.resolved_schema_cache.get(schema_name) {
            return Ok(resolved.clone());
        }

        let schema = self.get_schema_by_name(schema_name)?;
        let resolved = self.resolve_schema(&schema)?;

        self.resolved_schema_cache
            .insert(schema_name.to_string(), resolved.clone());
        Ok(resolved)
    }

    pub fn resolve_schema(&mut self, schema: &Value) -> Result<Value> {
        let expanded = self.expand_schema_references(schema)?;

        if get_schema_metadata(&expanded, "docs::hidden").is_some() {
            debug!("Instructed to skip resolution for the given schema.");
            return Ok(Value::Null); // Returning null to indicate skipped
        }

        if let Some(type_override) =
            get_schema_metadata(&expanded, "docs::type_override").and_then(|t| t.as_str())
        {
            let mut resolved = if type_override == "ascii_char" {
                if let Some(Value::Number(n)) = expanded.get("default") {
                    if let Some(c) = n.as_u64() {
                        #[allow(clippy::cast_possible_truncation)]
                        let c_char = (c as u8) as char;
                        json!({ "type": { type_override: { "default": c_char.to_string() } } })
                    } else {
                        json!({ "type": { type_override: {} } })
                    }
                } else {
                    json!({ "type": { type_override: {} } })
                }
            } else {
                json!({ "type": { type_override: {} } })
            };

            let desc = self.get_rendered_description_from_schema(&expanded);
            if !desc.is_empty() {
                resolved
                    .as_object_mut()
                    .unwrap()
                    .insert("description".to_string(), Value::String(desc));
            }
            return Ok(resolved);
        }

        let mut resolved = self.resolve_bare_schema(&expanded)?;
        if resolved.is_null() {
            return Ok(Value::Null);
        }

        // Remove description from array items
        if let Some(items_schema) = resolved.pointer_mut("/type/array/items")
            && let Value::Object(obj) = items_schema
        {
            obj.shift_remove("description");
        }

        self.apply_schema_default_value(&expanded, &mut resolved)?;
        self.apply_schema_metadata(&expanded, &mut resolved);

        let desc = self.get_rendered_description_from_schema(&expanded);
        if !desc.is_empty() {
            resolved
                .as_object_mut()
                .unwrap()
                .insert("description".to_string(), Value::String(desc));
        }

        if expanded
            .get("deprecated")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
        {
            resolved
                .as_object_mut()
                .unwrap()
                .insert("deprecated".to_string(), Value::Bool(true));
            if let Some(msg) = get_schema_metadata(&expanded, "deprecated_message") {
                resolved
                    .as_object_mut()
                    .unwrap()
                    .insert("deprecated_message".to_string(), msg.clone());
            }
        }

        if let Some(req) = get_schema_metadata(&expanded, "docs::required") {
            resolved
                .as_object_mut()
                .unwrap()
                .insert("required".to_string(), req.clone());
        }

        if let Some(warnings) = get_schema_metadata(&expanded, "docs::warnings") {
            let warnings_array = if let Some(arr) = warnings.as_array() {
                Value::Array(arr.clone())
            } else {
                Value::Array(vec![warnings.clone()])
            };
            resolved
                .as_object_mut()
                .unwrap()
                .insert("warnings".to_string(), warnings_array);
        }

        SchemaContext::reconcile_resolved_schema(&mut resolved);

        Ok(resolved)
    }

    #[allow(clippy::too_many_lines)]
    pub fn resolve_bare_schema(&mut self, schema: &Value) -> Result<Value> {
        let schema_type = self.get_json_schema_type(schema);

        let res = match schema_type {
            Some("all-of") => {
                debug!("Resolving composite schema.");
                if let Some(Value::Array(all_of)) = schema.get("allOf") {
                    let mut reduced = Value::Null;
                    for subschema in all_of {
                        let sub_res = self.resolve_schema(subschema)?;
                        if !sub_res.is_null() {
                            nested_merge(&mut reduced, &sub_res);
                        }
                    }
                    if reduced.is_null() {
                        return Ok(Value::Null);
                    }
                    reduced.get("type").cloned().unwrap_or(Value::Null)
                } else {
                    Value::Null
                }
            }
            Some("one-of" | "any-of") => {
                // Skip constraint schemas generated by required_one_of; they are not enum
                // definitions and would fail enum resolution.
                if schema
                    .get("_required_one_of_constraint")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false)
                {
                    return Ok(Value::Null);
                }
                debug!("Resolving enum schema.");
                let mut wrapped = self.resolve_enum_schema(schema)?;
                wrapped
                    .get_mut("_resolved")
                    .and_then(|r| r.get_mut("type"))
                    .cloned()
                    .unwrap_or(Value::Null)
            }
            Some("array") => {
                debug!("Resolving array schema.");
                let items_resolved = if let Some(items) = schema.get("items") {
                    self.resolve_schema(items)?
                } else {
                    Value::Null
                };
                json!({ "array": { "items": items_resolved } })
            }
            Some("object") => {
                debug!("Resolving object schema.");
                let properties = schema.get("properties").and_then(|p| p.as_object());
                let mut options = serde_json::Map::new();

                if let Some(props) = properties {
                    for (prop_name, prop_schema) in props {
                        debug!("Resolving object property '{prop_name}'...");
                        let mut resolved_property = self.resolve_schema(prop_schema)?;
                        if !resolved_property.is_null() {
                            self.apply_object_property_fields(
                                schema,
                                prop_schema,
                                prop_name,
                                &mut resolved_property,
                            );
                            options.insert(prop_name.clone(), resolved_property);
                        }
                    }
                }

                if let Some(addl_props) = schema.get("additionalProperties") {
                    debug!("Handling additional properties.");
                    if let Value::Object(mut map) = self.resolve_schema(addl_props)? {
                        map.insert("required".to_string(), Value::Bool(true));
                        if let Some(description) =
                            get_schema_metadata(schema, "docs::additional_props_description")
                        {
                            map.insert("description".to_string(), description.clone());
                        }
                        if map
                            .get("description")
                            .and_then(Value::as_str)
                            .is_none_or(|description| description.trim().is_empty())
                        {
                            bail!(
                                "Missing description for a wildcard field. Document its value type or provide 'docs::additional_props_description' metadata. Schema: {schema}"
                            );
                        }
                        options.insert("*".to_string(), Value::Object(map));
                    }
                }

                json!({ "object": { "options": options } })
            }
            Some("string") => {
                debug!("Resolving string schema.");
                let mut def = json!({});
                if let Some(d) = schema.get("default")
                    && !d.is_null()
                {
                    def.as_object_mut()
                        .unwrap()
                        .insert("default".to_string(), d.clone());
                }
                json!({ "string": def })
            }
            Some("number" | "integer") => {
                debug!("Resolving number schema.");
                let num_type = get_schema_metadata(schema, "docs::numeric_type")
                    .and_then(|n| n.as_str())
                    .unwrap_or("number");

                let mut def = json!({});
                if let Some(d) = schema.get("default")
                    && !d.is_null()
                {
                    def.as_object_mut()
                        .unwrap()
                        .insert("default".to_string(), d.clone());
                }
                json!({ num_type: def })
            }
            Some("boolean") => {
                debug!("Resolving boolean schema.");
                let mut def = json!({});
                if let Some(d) = schema.get("default")
                    && !d.is_null()
                {
                    def.as_object_mut()
                        .unwrap()
                        .insert("default".to_string(), d.clone());
                }
                json!({ "bool": def })
            }
            Some("const") => {
                debug!("Resolving const schema.");
                let const_val = schema.get("const").unwrap();
                let type_str = self.get_docs_type_for_value(Some(schema), const_val);

                let mut def = json!({ "value": const_val.clone() });
                let desc = self.get_rendered_description_from_schema(schema);
                if !desc.is_empty() {
                    def.as_object_mut()
                        .unwrap()
                        .insert("description".to_string(), Value::String(desc));
                }

                json!({ type_str: { "const": def } })
            }
            Some("enum") => {
                debug!("Resolving enum const schema.");
                if let Some(Value::Array(enum_vals)) = schema.get("enum") {
                    let mut grouped: indexmap::IndexMap<String, Vec<Value>> =
                        indexmap::IndexMap::new();
                    for val in enum_vals {
                        let t = docs_type_str(val);
                        grouped.entry(t.to_string()).or_default().push(val.clone());
                    }
                    self.fix_grouped_enums_if_numeric(&mut grouped);

                    let mut res = serde_json::Map::new();
                    for (k, v) in grouped {
                        let mut enum_map = serde_json::Map::new();
                        for item in v {
                            let key_str = item
                                .as_str()
                                .map_or_else(|| item.to_string(), std::string::ToString::to_string);
                            // Match the shape every other enum site emits: keys map to a
                            // string description (empty when the schema doesn't carry one).
                            enum_map.insert(key_str, Value::String(String::new()));
                        }
                        res.insert(k, json!({ "enum": enum_map }));
                    }
                    Value::Object(res)
                } else {
                    Value::Null
                }
            }
            None => {
                debug!("Resolving unconstrained schema.");
                json!({ "*": {} })
            }
            _ => bail!("Failed to resolve schema: {schema:?}"),
        };

        Ok(json!({ "type": res }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn context() -> SchemaContext {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| crate::app::set_global_verbosity(log::LevelFilter::Off));

        SchemaContext {
            root_schema: json!({
                "definitions": {
                    "Backend": {
                        "type": "object",
                        "title": "A documented backend.",
                        "description": "Request format:\n\n```json\n{\"version\": \"1.0\"}\n```",
                        "properties": {
                            "path": { "type": "string", "description": "Path to the secrets." }
                        }
                    }
                }
            }),
            cue_binary_path: String::new(),
            resolved_schema_cache: IndexMap::new(),
            expanded_schema_cache: IndexMap::new(),
        }
    }

    #[test]
    fn wildcard_inherits_referenced_value_description() {
        let resolved = context()
            .resolve_schema(&json!({
                "type": "object",
                "description": "All configured backends.",
                "additionalProperties": { "$ref": "#/definitions/Backend" }
            }))
            .unwrap();

        assert_eq!(resolved["description"], "All configured backends.");
        let entry = &resolved["type"]["object"]["options"]["*"];
        assert_eq!(
            entry["description"],
            "A documented backend.\n\nRequest format:\n\n```json\n{\"version\": \"1.0\"}\n```"
        );
        assert_eq!(entry["required"], true);
        assert_eq!(
            entry["type"]["object"]["options"]["path"]["description"],
            "Path to the secrets."
        );
    }

    #[test]
    fn wildcard_explicit_description_overrides_value_description() {
        for value_schema in [
            json!({ "$ref": "#/definitions/Backend" }),
            json!({ "type": "string" }),
        ] {
            let resolved = context()
                .resolve_schema(&json!({
                    "type": "object",
                    "_metadata": { "docs::additional_props_description": "A custom entry." },
                    "additionalProperties": value_schema
                }))
                .unwrap();

            assert_eq!(
                resolved["type"]["object"]["options"]["*"]["description"],
                "A custom entry."
            );
        }
    }

    #[test]
    fn wildcard_rejects_missing_value_description() {
        for value_schema in [
            json!({ "type": "string" }),
            json!({ "type": "string", "description": " \n " }),
        ] {
            let error = context()
                .resolve_schema(&json!({
                    "type": "object",
                    "description": "The map description does not describe an entry.",
                    "additionalProperties": value_schema
                }))
                .unwrap_err();

            assert!(
                error
                    .to_string()
                    .contains("Missing description for a wildcard field")
            );
        }
    }
}

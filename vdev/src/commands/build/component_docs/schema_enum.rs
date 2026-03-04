use super::{SchemaContext, get_schema_metadata, schema_aware_nested_merge};
use anyhow::Result;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};

impl SchemaContext {
    #[allow(clippy::too_many_lines)]
    pub fn resolve_enum_schema(&mut self, schema: &Value) -> Result<Value> {
        let mut subschemas = if let Some(one) = schema.get("oneOf") {
            one.as_array().unwrap().clone()
        } else if let Some(any) = schema.get("anyOf") {
            any.as_array().unwrap().clone()
        } else {
            error!(
                "Enum schema had both `oneOf` and `anyOf` specified (or neither). Schema: {}",
                schema
            );
            std::process::exit(1);
        };

        let is_optional = get_schema_metadata(schema, "docs::optional")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        subschemas.retain(|sub| {
            sub.get("type").and_then(|t| t.as_str()) != Some("null")
                && get_schema_metadata(sub, "docs::hidden").is_none()
        });

        let subschema_count = subschemas.len();

        if is_optional && subschema_count == 1 {
            let sub = &subschemas[0];
            if self.get_json_schema_type(sub) == Some("all-of") {
                debug!("Detected optional all-of schema, unwrapping all-of schema to resolve...");
                let mut unwrapped = schema.clone();
                let obj = unwrapped.as_object_mut().unwrap();
                obj.remove("oneOf");
                obj.remove("anyOf");
                obj.insert("allOf".to_string(), sub.get("allOf").unwrap().clone());
                return Ok(json!({ "_resolved": self.resolve_schema(&unwrapped)? }));
            }
            let mut unwrapped = schema.clone();
            let obj = unwrapped.as_object_mut().unwrap();
            obj.remove("oneOf");
            obj.remove("anyOf");
            schema_aware_nested_merge(&mut unwrapped, sub);
            return Ok(json!({ "_resolved": self.resolve_schema(&unwrapped)? }));
        }

        let enum_tagging =
            get_schema_metadata(schema, "docs::enum_tagging").and_then(|v| v.as_str());
        if enum_tagging.is_none() {
            error!(
                "Enum schemas should never be missing the metadata for the enum tagging mode. Schema: {}",
                schema
            );
            std::process::exit(1);
        }
        let enum_tagging = enum_tagging.unwrap();
        let enum_tag_field =
            get_schema_metadata(schema, "docs::enum_tag_field").and_then(|v| v.as_str());

        // Pattern: X or array of X
        if subschema_count == 2 {
            let array_idx = subschemas
                .iter()
                .position(|s| s.get("type").and_then(|t| t.as_str()) == Some("array"));
            if let Some(idx) = array_idx {
                debug!(
                    "Detected likely 'X or array of X' enum schema, applying further validation..."
                );
                let single_idx = usize::from(idx == 0);

                let single_reduced = self.get_reduced_schema(&subschemas[single_idx]);
                let array_reduced = self.get_reduced_schema(&subschemas[idx]);

                if Some(&single_reduced) == array_reduced.get("items") {
                    debug!("Reduced schemas match, fully resolving schema for X...");
                    let mut single_subschema = subschemas[single_idx].clone();
                    if self.get_json_schema_type(&single_subschema)
                        == schema.get("default").map(|d| super::json_type_str(d))
                    {
                        single_subschema.as_object_mut().unwrap().insert(
                            "default".to_string(),
                            schema.get("default").unwrap().clone(),
                        );
                    }

                    let resolved_subschema = self.resolve_schema(&single_subschema)?;
                    debug!("Resolved as 'X or array of X' enum schema.");
                    return Ok(
                        json!({ "_resolved": resolved_subschema, "annotations": "single_or_array" }),
                    );
                }
            }
        }

        // Pattern: simple internally tagged enum with named fields
        if enum_tagging == "internal" {
            debug!("Resolving enum subschemas to detect 'object'-ness...");
            let mut resolved_subschemas = Vec::new();
            for sub in &subschemas {
                let resolved = self.resolve_schema(sub)?;
                if self.resolved_schema_type(&resolved) == Some("object") {
                    resolved_subschemas.push(resolved);
                }
            }

            if resolved_subschemas.len() == subschema_count {
                debug!("Detected likely 'internally-tagged with named fields' enum schema...");
                let mut unique_resolved_properties = Map::new();
                let mut unique_tag_values: HashMap<String, Value> = HashMap::new();
                let tag_field = enum_tag_field.unwrap();

                for resolved_subschema in &mut resolved_subschemas {
                    let title = resolved_subschema.get("title").cloned();
                    let desc = resolved_subschema.get("description").cloned();

                    let opts = resolved_subschema
                        .pointer_mut("/type/object/options")
                        .unwrap()
                        .as_object_mut()
                        .unwrap();
                    let mut tag_subschema = opts.remove(tag_field).unwrap();

                    if let Some(t) = title {
                        tag_subschema
                            .as_object_mut()
                            .unwrap()
                            .insert("title".to_string(), t);
                    }
                    if let Some(d) = desc {
                        tag_subschema
                            .as_object_mut()
                            .unwrap()
                            .insert("description".to_string(), d);
                    }

                    let mut tag_value = None;
                    for allowed in ["string", "number", "integer", "boolean"] {
                        if let Some(const_val) =
                            tag_subschema.pointer(&format!("/type/{allowed}/const/value"))
                        {
                            if let Some(s) = const_val.as_str() {
                                tag_value = Some(s.to_string());
                                break;
                            } else {
                                tag_value = Some(const_val.to_string());
                                break;
                            }
                        }
                        if let Some(enum_vals) = tag_subschema
                            .pointer(&format!("/type/{allowed}/enum"))
                            .and_then(|v| v.as_object())
                            && let Some(first_key) = enum_vals.keys().next()
                        {
                            tag_value = Some(first_key.clone());
                            break;
                        }
                    }

                    if tag_value.is_none() {
                        error!(
                            "All enum subschemas representing an internally-tagged enum must have the tag field use a const value."
                        );
                        std::process::exit(1);
                    }
                    let tag_val_str = tag_value.unwrap();

                    if unique_tag_values.contains_key(&tag_val_str) {
                        error!(
                            "Found duplicate tag value '{}' when resolving enum subschemas.",
                            tag_val_str
                        );
                        std::process::exit(1);
                    }
                    unique_tag_values.insert(tag_val_str.clone(), tag_subschema.clone());

                    for (prop_name, prop_schema) in opts.iter_mut() {
                        if let Some(existing) = unique_resolved_properties.get_mut(prop_name) {
                            let reduced_existing = self.get_reduced_resolved_schema(existing);
                            let reduced_new = self.get_reduced_resolved_schema(prop_schema);
                            if reduced_existing != reduced_new {
                                error!(
                                    "Had overlapping property '{}' from resolved enum subschema, but schemas differed.",
                                    prop_name
                                );
                                std::process::exit(1);
                            }
                            existing
                                .get_mut("relevant_when")
                                .unwrap()
                                .as_array_mut()
                                .unwrap()
                                .push(Value::String(tag_val_str.clone()));
                        } else {
                            prop_schema
                                .as_object_mut()
                                .unwrap()
                                .insert("relevant_when".to_string(), json!([tag_val_str.clone()]));
                            unique_resolved_properties
                                .insert(prop_name.clone(), prop_schema.clone());
                        }
                    }
                }

                let unique_tags: HashSet<String> = unique_tag_values.keys().cloned().collect();
                for (_, val) in &mut unique_resolved_properties {
                    let val_obj = val.as_object_mut().unwrap();
                    if let Some(Value::Array(relevant)) = val_obj.get("relevant_when") {
                        let rel_set: HashSet<String> = relevant
                            .iter()
                            .map(|v| v.as_str().unwrap().to_string())
                            .collect();
                        if rel_set.len() == unique_tags.len() && rel_set == unique_tags {
                            val_obj.remove("relevant_when");
                        } else {
                            let mapped: Vec<String> = relevant
                                .iter()
                                .map(|v| format!("{tag_field} = {v}"))
                                .collect();
                            val_obj.insert(
                                "relevant_when".to_string(),
                                Value::String(mapped.join(" or ")),
                            );
                        }
                    }
                }

                let mut enum_vals = Map::new();
                for (k, v) in unique_tag_values {
                    let desc = self.get_rendered_description_from_schema(&v);
                    enum_vals.insert(k, Value::String(desc));
                }

                let mut resolved_tag_property_obj = Map::new();
                resolved_tag_property_obj.insert("required".to_string(), Value::Bool(true));
                resolved_tag_property_obj.insert(
                    "type".to_string(),
                    json!({ "string": { "enum": enum_vals } }),
                );

                let tag_desc = get_schema_metadata(schema, "docs::enum_tag_description");
                if tag_desc.is_none() {
                    error!(
                        "A unique tag description must be specified for enums which are internally tagged"
                    );
                    std::process::exit(1);
                }
                resolved_tag_property_obj
                    .insert("description".to_string(), tag_desc.unwrap().clone());

                unique_resolved_properties.insert(
                    tag_field.to_string(),
                    Value::Object(resolved_tag_property_obj),
                );

                return Ok(
                    json!({ "_resolved": { "type": { "object": { "options": unique_resolved_properties } } } }),
                );
            }
        }

        // ... remaining modes (external tagged unit variants, untagged narrowed, general fallback) ...

        // Return dummy for now, to be filled out.
        Ok(json!({ "_resolved": { "type": { "*": {} } } }))
    }
}

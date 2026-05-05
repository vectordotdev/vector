use super::{SchemaContext, get_schema_metadata, schema_aware_nested_merge};
use anyhow::{Result, bail};
use indexmap::IndexMap;
use serde_json::{Map, Value, json};
use std::collections::HashSet;

impl SchemaContext {
    #[allow(clippy::too_many_lines)]
    pub fn resolve_enum_schema(&mut self, schema: &Value) -> Result<Value> {
        let mut subschemas = match (schema.get("oneOf"), schema.get("anyOf")) {
            (Some(Value::Array(arr)), None) | (None, Some(Value::Array(arr))) => arr.clone(),
            _ => bail!(
                "Enum schema had both `oneOf` and `anyOf` specified (or neither). Schema: {schema}"
            ),
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
                obj.shift_remove("oneOf");
                obj.shift_remove("anyOf");
                obj.insert("allOf".to_string(), sub.get("allOf").unwrap().clone());
                return Ok(json!({ "_resolved": self.resolve_schema(&unwrapped)? }));
            }
            let mut unwrapped = schema.clone();
            let obj = unwrapped.as_object_mut().unwrap();
            obj.shift_remove("oneOf");
            obj.shift_remove("anyOf");
            schema_aware_nested_merge(&mut unwrapped, sub);
            return Ok(json!({ "_resolved": self.resolve_schema(&unwrapped)? }));
        }

        let Some(enum_tagging) =
            get_schema_metadata(schema, "docs::enum_tagging").and_then(|v| v.as_str())
        else {
            bail!(
                "Enum schemas should never be missing the metadata for the enum tagging mode. Schema: {schema}"
            );
        };
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
                let mut unique_tag_values: IndexMap<String, Value> = IndexMap::new();
                let tag_field = enum_tag_field.unwrap();

                for resolved_subschema in &mut resolved_subschemas {
                    let title = resolved_subschema.get("title").cloned();
                    let desc = resolved_subschema.get("description").cloned();

                    let opts = resolved_subschema
                        .pointer_mut("/type/object/options")
                        .unwrap()
                        .as_object_mut()
                        .unwrap();
                    let mut tag_subschema = opts.shift_remove(tag_field).unwrap();

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
                            } else {
                                tag_value = Some(const_val.to_string());
                            }
                            break;
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

                    let Some(tag_val_str) = tag_value else {
                        bail!(
                            "All enum subschemas representing an internally-tagged enum must have the tag field use a const value. Tag field: '{tag_field}', subschema: {tag_subschema}"
                        );
                    };

                    if unique_tag_values.contains_key(&tag_val_str) {
                        bail!(
                            "Found duplicate tag value '{tag_val_str}' when resolving enum subschemas. Tag field: '{tag_field}'."
                        );
                    }
                    unique_tag_values.insert(tag_val_str.clone(), tag_subschema.clone());

                    for (prop_name, prop_schema) in opts.iter_mut() {
                        if let Some(existing) = unique_resolved_properties.get_mut(prop_name) {
                            let reduced_existing = self.get_reduced_resolved_schema(existing);
                            let reduced_new = self.get_reduced_resolved_schema(prop_schema);
                            if reduced_existing != reduced_new {
                                bail!(
                                    "Had overlapping property '{prop_name}' from resolved enum subschema, but schemas differed. Existing: {reduced_existing}, new: {reduced_new}."
                                );
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
                            val_obj.shift_remove("relevant_when");
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

                let Some(tag_desc) = get_schema_metadata(schema, "docs::enum_tag_description")
                else {
                    bail!(
                        "A unique tag description must be specified for enums which are internally tagged. Schema: {schema}"
                    );
                };
                resolved_tag_property_obj.insert("description".to_string(), tag_desc.clone());

                unique_resolved_properties.insert(
                    tag_field.to_string(),
                    Value::Object(resolved_tag_property_obj),
                );

                return Ok(
                    json!({ "_resolved": { "type": { "object": { "options": unique_resolved_properties } } } }),
                );
            }
        }

        // Schema pattern: simple externally tagged enum with only unit variants.
        if enum_tagging == "external" {
            let mut tag_values: IndexMap<String, Value> = IndexMap::new();
            let mut all_const_strings = true;

            for subschema in &subschemas {
                if let Some(const_val) = subschema.get("const") {
                    if let Some(s) = const_val.as_str() {
                        tag_values.insert(s.to_string(), subschema.clone());
                    } else {
                        all_const_strings = false;
                        break;
                    }
                } else {
                    all_const_strings = false;
                    break;
                }
            }

            if all_const_strings && !tag_values.is_empty() {
                debug!("Resolved as 'externally-tagged with only unit variants' enum schema.");
                let mut enum_vals = Map::new();
                for (k, v) in tag_values {
                    let desc = self.get_rendered_description_from_schema(&v);
                    enum_vals.insert(k, Value::String(desc));
                }
                return Ok(json!({ "_resolved": { "type": { "string": { "enum": enum_vals } } } }));
            }
        }

        // Schema pattern: untagged enum with narrowing constant variants and catch-all free-form variant.
        if enum_tagging == "untagged" {
            let mut type_def_kinds: Vec<String> = Vec::new();
            let mut fixed_subschemas = 0;
            let mut freeform_subschemas = 0;

            for subschema in &subschemas {
                let schema_type = self.get_json_schema_type(subschema);
                match schema_type {
                    None | Some("all-of" | "one-of") => {
                        // We don't handle these cases.
                    }
                    Some("const") => {
                        if let Some(const_val) = subschema.get("const") {
                            type_def_kinds.push(super::docs_type_str(const_val).to_string());
                        }
                        fixed_subschemas += 1;
                    }
                    Some("enum") => {
                        if let Some(Value::Array(enum_vals)) = subschema.get("enum") {
                            for val in enum_vals {
                                type_def_kinds.push(super::docs_type_str(val).to_string());
                            }
                        }
                        fixed_subschemas += 1;
                    }
                    Some(t) => {
                        type_def_kinds.push(t.to_string());
                        freeform_subschemas += 1;
                    }
                }
            }

            let unique_kinds: HashSet<_> = type_def_kinds.iter().collect();
            if unique_kinds.len() == 1 && fixed_subschemas >= 1 && freeform_subschemas == 1 {
                debug!("Resolved as 'untagged with narrowed free-form' enum schema.");
                let type_def_kind = type_def_kinds.first().unwrap();
                return Ok(
                    json!({ "_resolved": { "type": { type_def_kind: {} } }, "annotations": "narrowed_free_form" }),
                );
            }
        }

        // Schema pattern: simple externally tagged enum with only non-unit variants.
        if enum_tagging == "external" {
            let all_objects = subschemas
                .iter()
                .all(|s| self.get_json_schema_type(s) == Some("object"));

            if all_objects {
                let mut aggregated_properties = Map::new();

                for subschema in &subschemas {
                    let resolved_subschema = self.resolve_schema(subschema)?;
                    if let Some(Value::Object(resolved_properties)) =
                        resolved_subschema.pointer("/type/object/options")
                    {
                        if resolved_properties.len() != 1 {
                            bail!(
                                "Expected exactly 1 property for externally-tagged non-unit enum variant, got {len}. Schema: {subschema}",
                                len = resolved_properties.len()
                            );
                        }
                        let description = self.get_rendered_description_from_schema(subschema);
                        for (property_name, property_schema) in resolved_properties {
                            let mut prop = property_schema.clone();
                            if !description.is_empty() {
                                prop.as_object_mut().unwrap().insert(
                                    "description".to_string(),
                                    Value::String(description.clone()),
                                );
                            }
                            aggregated_properties.insert(property_name.clone(), prop);
                        }
                    }
                }

                if !aggregated_properties.is_empty() {
                    debug!(
                        "Resolved as 'externally-tagged with only non-unit variants' enum schema."
                    );
                    return Ok(
                        json!({ "_resolved": { "type": { "object": { "options": aggregated_properties } } } }),
                    );
                }
            }
        }

        // Fallback schema pattern: mixed-mode enums.
        debug!("Resolved as 'fallback mixed-mode' enum schema.");
        debug!("Tagging mode: {}", enum_tagging);

        let mut resolved_subschemas: Vec<Value> = Vec::new();
        for subschema in &subschemas {
            let resolved = self.resolve_schema(subschema)?;
            if !resolved.is_null() {
                resolved_subschemas.push(resolved);
            }
        }

        if resolved_subschemas.is_empty() {
            return Ok(json!({ "_resolved": { "type": { "*": {} } } }));
        }

        let mut type_defs = resolved_subschemas[0].clone();
        for item in resolved_subschemas.iter().skip(1) {
            schema_aware_nested_merge(&mut type_defs, item);
        }

        let mut merged_type = type_defs
            .get("type")
            .cloned()
            .unwrap_or_else(|| json!({ "*": {} }));

        if let Value::Object(type_map) = &mut merged_type {
            for (_, type_def) in type_map.iter_mut() {
                if let Value::Object(def) = type_def
                    && let Some(Value::Array(const_arr)) = def.shift_remove("const")
                {
                    let mut enum_map = Map::new();
                    for const_obj in &const_arr {
                        if let Some(value) = const_obj.get("value").and_then(|v| v.as_str()) {
                            let desc = self.get_rendered_description_from_schema(const_obj);
                            enum_map.insert(value.to_string(), Value::String(desc));
                        }
                    }
                    if !enum_map.is_empty() {
                        def.insert("enum".to_string(), Value::Object(enum_map));
                    }
                }
            }
        }

        Ok(json!({ "_resolved": { "type": merged_type }, "annotations": "mixed_mode" }))
    }
}

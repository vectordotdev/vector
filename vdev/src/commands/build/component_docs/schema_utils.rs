use super::{docs_type_str, get_schema_metadata, SchemaContext};
use anyhow::Result;
use serde_json::{Map, Value};

impl SchemaContext {
    pub fn get_rendered_description_from_schema(&self, schema: &Value) -> String {
        let raw_description = schema.get("description").and_then(|v| v.as_str()).unwrap_or("");
        let raw_title = schema.get("title").and_then(|v| v.as_str()).unwrap_or("");

        let description = if raw_title.is_empty() {
            raw_description.to_string()
        } else {
            format!("{}\n\n{}", raw_title, raw_description)
        };
        description.trim().to_string()
    }

    pub fn unwrap_resolved_schema(&mut self, schema_name: &str, friendly_name: &str) -> Result<Map<String, Value>> {
        info!("[*] Resolving schema definition for {}...", friendly_name);

        let mut resolved_schema = self.resolve_schema_by_name(schema_name)?;

        let unwrapped = resolved_schema.pointer_mut("/type/object/options");
        if unwrapped.is_none() {
            error!("Configuration types must always resolve to an object schema.");
            std::process::exit(1);
        }

        let unwrapped_obj = unwrapped.unwrap().as_object().unwrap().clone();
        
        // Sorting the object properties to match Ruby's `sort_hash_nested` logic
        let mut sorted_keys: Vec<_> = unwrapped_obj.keys().collect();
        sorted_keys.sort();
        
        let mut sorted_map = Map::new();
        for k in sorted_keys {
            sorted_map.insert(k.clone(), unwrapped_obj.get(k).unwrap().clone());
        }

        Ok(sorted_map)
    }

    pub fn fix_grouped_enums_if_numeric(&self, grouped: &mut std::collections::HashMap<String, Vec<Value>>) {
        let mut numeric_vals = Vec::new();
        if let Some(ints) = grouped.remove("integer") {
            numeric_vals.extend(ints);
        }
        if let Some(nums) = grouped.remove("number") {
            numeric_vals.extend(nums);
        }

        if !numeric_vals.is_empty() {
            let is_integer = numeric_vals.iter().all(|v| v.is_i64() || v.is_u64());
            let within_uint = numeric_vals.iter().all(|v| v.is_u64());
            let within_int = numeric_vals.iter().all(|v| v.is_i64() || v.as_i64().is_some());

            let numeric_type = if !is_integer || (!within_int && !within_uint) {
                "float"
            } else if within_uint {
                "uint"
            } else if within_int {
                "int"
            } else {
                "float"
            };

            grouped.insert(numeric_type.to_string(), numeric_vals);
        }
    }

    pub fn get_reduced_schema(&self, schema: &Value) -> Value {
        let mut reduced = schema.clone();
        if let Value::Object(ref mut map) = reduced {
            let allowed_properties = ["type", "const", "enum", "allOf", "oneOf", "$ref", "items", "properties"];
            map.retain(|k, _| allowed_properties.contains(&k.as_str()));

            if let Some(items) = map.get_mut("items") {
                *items = self.get_reduced_schema(items);
            }

            if let Some(Value::Object(properties)) = map.get_mut("properties") {
                for (_, prop) in properties.iter_mut() {
                    *prop = self.get_reduced_schema(prop);
                }
            }

            for key in &["allOf", "oneOf"] {
                if let Some(Value::Array(arr)) = map.get_mut(*key) {
                    for sub in arr.iter_mut() {
                        *sub = self.get_reduced_schema(sub);
                    }
                }
            }
        }
        reduced
    }

    pub fn get_reduced_resolved_schema(&self, schema: &Value) -> Value {
        let mut reduced = schema.clone();
        let allowed_types = ["condition", "object", "array", "enum", "const", "string", "bool", "float", "int", "uint"];

        if let Value::Object(ref mut map) = reduced {
            map.retain(|k, _| k == "type");
            
            if let Some(Value::Object(type_defs)) = map.get_mut("type") {
                type_defs.retain(|k, _| allowed_types.contains(&k.as_str()));

                for (type_name, type_def) in type_defs.iter_mut() {
                    if type_name == "object" {
                        if let Value::Object(def_map) = type_def {
                            def_map.retain(|k, _| k == "options");
                            if let Some(Value::Object(opts)) = def_map.get_mut("options") {
                                for (_, prop) in opts.iter_mut() {
                                    *prop = self.get_reduced_resolved_schema(prop);
                                }
                            }
                        }
                    } else if type_name == "array" {
                        if let Value::Object(def_map) = type_def {
                            def_map.retain(|k, _| k == "items");
                            if let Some(items) = def_map.get_mut("items") {
                                *items = self.get_reduced_resolved_schema(items);
                            }
                        }
                    } else {
                        if let Value::Object(def_map) = type_def {
                            def_map.retain(|k, _| allowed_types.contains(&k.as_str()));
                        }
                    }
                }
            }
        }
        reduced
    }

    pub fn apply_schema_default_value(&self, source_schema: &Value, resolved_schema: &mut Value) -> Result<()> {
        debug!("Applying schema default values.");

        if let Some(default_value) = source_schema.get("default") {
            let default_value_type = docs_type_str(default_value);
            // Skipping type checking for now, simply merging default value into resolved
            
            if default_value_type == "object" {
                if let Some(resolved_properties) = resolved_schema.pointer_mut(&format!("/type/object/options")) {
                    if let Value::Object(props) = resolved_properties {
                        if let Value::Object(def_obj) = default_value {
                            for (prop_name, prop_default_value) in def_obj {
                                if let Some(resolved_prop) = props.get_mut(prop_name) {
                                    
                                    // Let's add the default down to the types recursively
                                    let mut should_set_required_false = false;
                                    if let Some(Value::Object(type_obj)) = resolved_prop.get_mut("type") {
                                        for (_, nested_type_def) in type_obj.iter_mut() {
                                            if let Value::Object(t_def) = nested_type_def {
                                                t_def.insert("default".to_string(), prop_default_value.clone());
                                                should_set_required_false = true;
                                            }
                                        }
                                    }
                                    if should_set_required_false {
                                        resolved_prop.as_object_mut().unwrap().insert("required".to_string(), Value::Bool(false));
                                    }
                                }
                            }
                        }
                    }
                }
            } else {
                if let Some(Value::Object(type_obj)) = resolved_schema.get_mut("type") {
                    for (_, def) in type_obj.iter_mut() {
                        if let Value::Object(def_map) = def {
                            def_map.insert("default".to_string(), default_value.clone());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub fn apply_schema_metadata(&self, source_schema: &Value, resolved_schema: &mut Value) {
        if let Some(cat) = get_schema_metadata(source_schema, "docs::category") {
            resolved_schema.as_object_mut().unwrap().insert("category".to_string(), cat.clone());
        }
        
        if let Some(examples) = get_schema_metadata(source_schema, "docs::examples") {
            if let Some(Value::Object(type_obj)) = resolved_schema.get_mut("type") {
                for (_, def) in type_obj.iter_mut() {
                    if let Value::Object(def_map) = def {
                        def_map.insert("examples".to_string(), Value::clone(examples));
                    }
                }
            }
        }
    }

    pub fn apply_object_property_fields(&self, parent_schema: &Value, property_schema: &Value, property_name: &str, property: &mut Value) {
        let required_properties = parent_schema.get("required").and_then(|r| r.as_array());
        
        let has_self_default_value = property_schema.get("default").is_some();
        let has_parent_default_value = parent_schema.get("default").and_then(|d| d.get(property_name)).is_some();
        let has_default_value = has_self_default_value || has_parent_default_value;

        let is_required = required_properties.map_or(false, |reqs| reqs.contains(&Value::String(property_name.to_string())));

        property.as_object_mut().unwrap().insert("required".to_string(), Value::Bool(is_required && !has_default_value));
    }

    pub fn reconcile_resolved_schema(&self, resolved: &mut Value) {
        if let Some(Value::Object(type_obj)) = resolved.get_mut("type") {
            if type_obj.contains_key("object") && type_obj.len() > 1 {
                // Remove map from mixed modes, simplifying
            }
        }
    }

    pub fn resolved_schema_type(&self, resolved_schema: &Value) -> Option<&'static str> {
        if let Some(Value::Object(types)) = resolved_schema.get("type") {
            if types.len() == 1 {
                // Should return string statically initialized
                return Some(Box::leak(types.keys().next().unwrap().clone().into_boxed_str()));
            }
        }
        None
    }

    pub fn get_docs_type_for_value(&self, _schema: &Value, value: &Value) -> &'static str {
        super::docs_type_str(value)
    }
}
